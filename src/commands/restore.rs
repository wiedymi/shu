//! Commands that materialize catalog entries as local repositories.

use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

use anyhow::{Context, Result, anyhow, bail};

use crate::{
    catalog::{self, catalog_path, repo_name},
    cli::{
        Cli, EnsureArgs, FilterArgs, RestoreArgs, SelectorArgs, SyncArgs, SyncCommand,
        SyncInitArgs, UpdateArgs,
    },
    git, locations,
    model::{Catalog, Sync},
    paths::{absolute, repo_path, root_path},
    sources,
};

/// Restore a catalog source, if supplied, and clone all selected missing entries.
pub fn restore(cli: &Cli, args: &RestoreArgs) -> Result<()> {
    if let Some(source) = &args.source {
        activate_source(cli, args, source)?;
    }
    let (catalog_path, mut catalog) = catalog::load_or_initialize(cli)?;
    let repo_indices = catalog
        .repos
        .iter()
        .enumerate()
        .filter(|(_, repo)| {
            args.filter.state.is_none_or(|state| repo.state == state)
                && args
                    .filter
                    .tag
                    .as_ref()
                    .is_none_or(|tag| repo.tags.contains(tag))
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if !confirm_restore(cli, args, repo_indices.len())? {
        println!("No changes made.");
        return Ok(());
    }
    fs::create_dir_all(root_path(&catalog)?)?;
    let mut failures = 0;
    let mut changed = false;
    for index in repo_indices {
        let source = catalog.repos[index].source.clone();
        match restore_one(&mut catalog, index) {
            Ok(wrote_path) => changed |= wrote_path,
            Err(error) => {
                eprintln!("! {source}: {error:#}");
                failures += 1;
            }
        }
    }
    if changed {
        catalog::save(&catalog_path, &catalog)?;
    }
    if failures > 0 {
        bail!(
            "restore completed with {failures} failure(s); repositories that were accessible were restored. Review the messages above, then check the affected path, internet connection, or Git access."
        );
    }
    Ok(())
}

/// Refresh the configured Git catalog, then restore newly missing repositories.
pub fn update(cli: &Cli, args: &UpdateArgs) -> Result<()> {
    if args.selector.is_some() || args.state.is_some() || args.note.is_some() {
        let selector = args.selector.as_deref().unwrap_or("<repository>");
        bail!(
            "`shu update` refreshes the saved catalog source; it does not edit a repository. Use `shu edit {selector} --state <state> --note <text>` instead"
        );
    }
    let (_, catalog) = catalog::load(cli)?;
    let sync = catalog.sync.ok_or_else(|| {
        anyhow!(
            "no Git catalog is configured; add a [sync] table to shu.toml, then run `shu restore {remote}`",
            remote = "<remote>"
        )
    })?;
    restore(
        cli,
        &RestoreArgs {
            source: Some(sync.remote),
            file: Some(PathBuf::from(sync.file)),
            git_ref: Some(sync.r#ref),
            filter: FilterArgs::default(),
        },
    )
}

/// Safely commit and push the active catalog through its persistent Git checkout.
pub fn sync(cli: &Cli, args: &SyncArgs) -> Result<()> {
    match &args.command {
        Some(SyncCommand::Init(args)) => sync_init(cli, args),
        None => sync_catalog(cli),
    }
}

/// Safely commit and push the active catalog through its persistent Git checkout.
fn sync_catalog(cli: &Cli) -> Result<()> {
    let (catalog_file, catalog) = catalog::load(cli)?;
    let sync = catalog.sync.clone().ok_or_else(|| {
        anyhow!("no Git catalog is configured; add [sync] to shu.toml before running `shu sync`")
    })?;
    let checkout = sync_checkout(&catalog, &sync)?;
    verify_checkout(&checkout, &sync)?;
    let filename = sync_filename(&sync)?;
    git::output(&checkout, ["fetch", "origin", &sync.r#ref])?;
    let local_revision = git::output(&checkout, ["rev-parse", "HEAD"])?;
    let remote_revision = git::output(&checkout, ["rev-parse", "FETCH_HEAD"])?;
    if local_revision != remote_revision {
        bail!(
            "catalog source changed remotely; run `shu restore {}` before syncing",
            sync.remote
        );
    }
    let remote_catalog = checkout.join(&filename);
    if !remote_catalog.is_file() {
        bail!(
            "catalog file {} is missing from the remote source",
            filename.display()
        );
    }
    let local_content = fs::read_to_string(&catalog_file)?;
    if fs::read_to_string(&remote_catalog)? == local_content {
        println!("Catalog is already synced.");
        return Ok(());
    }
    fs::write(&remote_catalog, local_content)?;
    let filename = filename
        .to_str()
        .ok_or_else(|| anyhow!("catalog filename is not valid UTF-8"))?;
    git::output(&checkout, ["add", "--", filename])?;
    git::output(&checkout, ["commit", "-m", "Sync Shu catalog"])?;
    let branch = git::output(&checkout, ["symbolic-ref", "--short", "HEAD"])
        .context("catalog source must use a branch, not a detached ref")?;
    if branch != sync.r#ref {
        bail!(
            "catalog checkout is on {branch}, but [sync].ref is {}; run `shu restore {}`",
            sync.r#ref,
            sync.remote
        );
    }
    git::output(&checkout, ["push", "origin", &sync.r#ref])?;
    println!("Synced catalog to {}.", sync.remote);
    Ok(())
}

/// Create a dedicated catalog repository without treating it as a user project.
fn sync_init(cli: &Cli, args: &SyncInitArgs) -> Result<()> {
    let (active_path, active_catalog) = catalog::load_or_initialize(cli)?;
    if active_catalog.sync.is_some() {
        bail!("a Git catalog is already configured; use `shu sync` or `shu update`");
    }
    let remote = sources::repository_remote(&args.source)?
        .ok_or_else(|| anyhow!("`shu sync init` requires a Git remote or repository identity"))?;
    let identity = crate::identity::normalize_identity(&remote)?;
    if args.github && !identity.starts_with("github.com/") {
        bail!("--github requires a github.com/owner/repository identity");
    }
    if args.github {
        super::catalog::ensure_github_ready()?;
    }
    let sync = Sync {
        remote: remote.clone(),
        file: "shu.toml".into(),
        r#ref: "main".into(),
    };
    let checkout = sync_checkout(&active_catalog, &sync)?;
    if checkout.exists() {
        bail!(
            "catalog checkout already exists: {}. Use `shu restore {remote}` instead",
            checkout.display()
        );
    }
    git::initialize(&checkout)?;
    if args.github {
        super::catalog::create_github_repository(&checkout, &identity, args.private)?;
    } else {
        git::output(&checkout, ["remote", "add", "origin", &remote])?;
    }
    let mut synced_catalog = active_catalog;
    synced_catalog.sync = Some(sync.clone());
    let catalog_file = checkout.join(sync_filename(&sync)?);
    catalog::save(&catalog_file, &synced_catalog)?;
    git::output(&checkout, ["add", "--", "shu.toml"])?;
    git::output(&checkout, ["commit", "-m", "Initialize Shu catalog"])
        .context("could not commit the catalog; configure your Git author identity first")?;
    git::output(&checkout, ["push", "-u", "origin", "main"]).context(
        "could not publish the catalog; confirm the remote exists and Git access is configured",
    )?;
    catalog::save(&active_path, &synced_catalog)?;
    println!("Initialized catalog sync at {remote}.");
    println!("  Checkout: {}", checkout.display());
    Ok(())
}

/// Validate the catalog path before reading or writing inside its checkout.
fn sync_filename(sync: &Sync) -> Result<PathBuf> {
    let file = PathBuf::from(&sync.file);
    if file.is_absolute()
        || file
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        bail!("catalog source contains an unsafe catalog filename");
    }
    Ok(file)
}

/// Return the persistent, inspectable checkout for a synced catalog.
fn sync_checkout(catalog: &Catalog, sync: &Sync) -> Result<PathBuf> {
    Ok(root_path(catalog)?.join(crate::identity::normalize_identity(&sync.remote)?))
}

/// Refuse to use a checkout that differs from the declared catalog source.
fn verify_checkout(checkout: &std::path::Path, sync: &Sync) -> Result<()> {
    if !git::is_repo(checkout) {
        bail!(
            "catalog checkout is missing at {}; run `shu restore {}`",
            checkout.display(),
            sync.remote
        );
    }
    if !git::is_clean(checkout)? {
        bail!(
            "catalog checkout has local changes: {}; commit, stash, or discard them before syncing",
            checkout.display()
        );
    }
    // `git remote get-url` expands url.*.insteadOf rules. Read the stored
    // value instead so transport rewrites do not change the source identity.
    let configured_remote = git::output(checkout, ["config", "--get", "remote.origin.url"])?;
    if crate::identity::normalize_identity(&configured_remote)?
        != crate::identity::normalize_identity(&sync.remote)?
    {
        bail!("catalog checkout origin does not match [sync].remote");
    }
    Ok(())
}

/// Ensure one selected repository exists and print its absolute local path.
pub fn ensure(cli: &Cli, args: &EnsureArgs) -> Result<()> {
    let (catalog_path, mut catalog) = catalog::load_or_initialize(cli)?;
    let index = catalog::select_index(&catalog, &args.selector)?;
    let name = repo_name(&catalog.repos[index]).to_owned();
    let (target, cloned) = materialize(&mut catalog, index)?;
    if cloned {
        catalog::save(&catalog_path, &catalog)?;
    }
    if args.path_only {
        println!("{}", target.display());
    } else {
        println!("Ensured {name} at {}", target.display());
    }
    Ok(())
}

/// Print a selected repository's path only when it already exists locally.
pub fn path_command(cli: &Cli, args: &SelectorArgs) -> Result<()> {
    let (_, catalog) = catalog::load_or_initialize(cli)?;
    let repo = catalog::select(&catalog, &args.selector)?;
    let Some(path) = locations::present_path(&catalog, repo)? else {
        bail!(
            "repository is missing locally: {}",
            locations::managed_path(&catalog, repo)?.display()
        );
    };
    println!("{}", path.display());
    Ok(())
}

/// Resolve, validate, and persist a catalog source before restoration.
fn activate_source(cli: &Cli, args: &RestoreArgs, source: &str) -> Result<()> {
    if let Some(remote) = sources::repository_remote(source)? {
        return activate_git_source(cli, args, &remote);
    }
    let content = sources::resolve(source, args.file.as_deref(), args.git_ref.as_deref())?;
    let loaded: Catalog = toml::from_str(&content).context("resolved catalog is not valid TOML")?;
    if loaded.version != 1 {
        bail!("unsupported catalog version {}", loaded.version);
    }
    catalog::save(&catalog_path(cli)?, &loaded)?;
    eprintln!("Using catalog from {source}");
    Ok(())
}

/// Clone or fast-forward the normal catalog checkout, then activate its TOML file.
fn activate_git_source(cli: &Cli, args: &RestoreArgs, remote: &str) -> Result<()> {
    let (_, active) = catalog::load_or_initialize(cli)?;
    let file = args
        .file
        .as_deref()
        .unwrap_or_else(|| std::path::Path::new("shu.toml"));
    let reference = args.git_ref.as_deref().unwrap_or("main");
    let provisional = Sync {
        remote: remote.to_owned(),
        file: file.display().to_string(),
        r#ref: reference.to_owned(),
    };
    let checkout = sync_checkout(&active, &provisional)?;
    if checkout.exists() {
        verify_checkout(&checkout, &provisional)?;
        git::output(&checkout, ["pull", "--ff-only", "origin", reference])?;
    } else {
        git::clone_remote(remote, &checkout, Some(reference))?;
    }
    let catalog_file = checkout.join(sync_filename(&provisional)?);
    let loaded: Catalog =
        toml::from_str(&fs::read_to_string(&catalog_file).with_context(|| {
            format!("catalog file {} was not found in {remote}", file.display())
        })?)
        .context("resolved catalog is not valid TOML")?;
    if loaded.version != 1 {
        bail!("unsupported catalog version {}", loaded.version);
    }
    let sync = loaded.sync.as_ref().ok_or_else(|| {
        anyhow!(
            "catalog file {} needs a [sync] table before it can be restored from Git",
            file.display()
        )
    })?;
    if crate::identity::normalize_identity(&sync.remote)?
        != crate::identity::normalize_identity(remote)?
        || sync.file != provisional.file
        || sync.r#ref != provisional.r#ref
    {
        bail!("[sync] must match the repository URL, file, and ref passed to `shu restore`");
    }
    catalog::save(&catalog_path(cli)?, &loaded)?;
    eprintln!("Using catalog from {remote}");
    Ok(())
}

/// Request confirmation only when a new source may cause cloning.
///
/// Returns `false` for an explicit cancellation so cancellation remains a
/// successful, non-mutating command outcome.
fn confirm_restore(cli: &Cli, args: &RestoreArgs, count: usize) -> Result<bool> {
    if args.source.is_none() || cli.yes || cli.non_interactive {
        return Ok(true);
    }
    eprint!("Catalog contains {count} repositories. Continue? [Y/n] ");
    io::stderr().flush()?;
    let mut response = String::new();
    io::stdin().read_line(&mut response)?;
    if response.trim().is_empty()
        || response.trim().eq_ignore_ascii_case("y")
        || response.trim().eq_ignore_ascii_case("yes")
    {
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Clone one missing repository while preserving any existing path conflict.
fn restore_one(catalog: &mut Catalog, index: usize) -> Result<bool> {
    let name = repo_name(&catalog.repos[index]).to_owned();
    let (path, cloned) = materialize(catalog, index)?;
    if !cloned {
        println!("✓ {} already present at {}", name, path.display());
    }
    Ok(cloned)
}

/// Return an existing local clone or create Shu's canonical clone and remember it.
///
/// The caller owns catalog persistence because batch restore saves only once.
pub(crate) fn materialize(catalog: &mut Catalog, index: usize) -> Result<(PathBuf, bool)> {
    let repo = &catalog.repos[index];
    if let Some(path) = locations::present_path(catalog, repo)? {
        return Ok((path, false));
    }
    let target = repo_path(catalog, repo)?;
    if target.exists() {
        bail!(
            "{} exists but is not a valid Git repository: {}",
            repo_name(repo),
            target.display()
        );
    } else {
        eprintln!("↓ cloning {}", repo.source);
        git::clone(&repo.source, &target)?;
        let target = absolute(&target)?;
        remember_new_clone(&mut catalog.repos[index], &target);
        Ok((target, true))
    }
}

/// Add a just-created canonical clone to the one catalog file and prefer it if needed.
fn remember_new_clone(repo: &mut crate::model::Repo, path: &std::path::Path) {
    let replace_primary = repo
        .primary_path()
        .is_none_or(|primary| !git::is_repo(&primary));
    repo.add_path(path.to_path_buf());
    if replace_primary {
        repo.primary = Some(path.display().to_string());
    }
}
