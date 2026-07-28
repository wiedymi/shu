//! Commands that create, inspect, and edit catalog entries.

use std::{
    collections::HashSet,
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use crossterm::terminal;
use walkdir::{DirEntry, WalkDir};

use crate::{
    catalog::{self, catalog_path},
    cli::{AddArgs, Cli, EditArgs, NewArgs, ScanArgs, SelectorArgs},
    commands::restore,
    git,
    identity::normalize_identity,
    locations,
    model::{Catalog, Lifecycle, Repo},
    paths::{absolute, root_path},
    ui,
};

/// Initialize an empty catalog without overwriting an existing one.
pub fn init(cli: &Cli) -> Result<()> {
    let path = catalog_path(cli)?;
    if path.exists() {
        bail!(
            "catalog already exists at {}; use --catalog to choose another path",
            path.display()
        );
    }
    catalog::save(
        &path,
        &Catalog {
            version: 1,
            root: crate::model::default_root(),
            repos: vec![],
            sync: None,
        },
    )?;
    println!("Initialized Shu catalog at {}", path.display());
    Ok(())
}

/// Add a repository to the active catalog, cloning remote identities when absent.
pub fn add(cli: &Cli, args: &AddArgs) -> Result<()> {
    let (path, mut catalog) = catalog::load_or_initialize(cli)?;
    if args.migrate {
        return migrate_and_add(cli, args, &path, &mut catalog);
    }
    let source = catalog::source_from_argument(&args.source)?;
    let identity = normalize_identity(&source)?;
    let remote = ssh_transport(&source);
    let local_path = local_source_path(&args.source)?;
    if let Some(index) = existing_repo_index(&catalog, &identity) {
        if let Some(local_path) = local_path {
            if catalog.repos[index].remote.is_none() {
                catalog.repos[index].remote = remote;
            }
            remember_path(&mut catalog, index, &local_path)?;
            let name = catalog::repo_name(&catalog.repos[index]).to_owned();
            catalog::save(&path, &catalog)?;
            println!("Already catalogued {name}");
            println!("  Local clone: {}", local_path.display());
            return Ok(());
        }
        let name = catalog::repo_name(&catalog.repos[index]).to_owned();
        let (local_path, cloned) = restore::materialize(&mut catalog, index)?;
        if cloned {
            catalog::save(&path, &catalog)?;
        }
        println!("Already catalogued {name}");
        println!("  Local clone: {}", local_path.display());
        return Ok(());
    }
    add_entry(&mut catalog, args, &identity, remote);
    if let Some(local_path) = &local_path {
        let index = catalog.repos.len() - 1;
        remember_path(&mut catalog, index, local_path)?;
    }
    let index = catalog.repos.len() - 1;
    let local_path = match local_path {
        Some(local_path) => local_path,
        None => restore::materialize(&mut catalog, index)?.0,
    };
    catalog::save(&path, &catalog)?;
    println!("Added {identity}");
    println!("  Local clone: {}", local_path.display());
    Ok(())
}

/// Create and catalogue an empty local repository at Shu's canonical path.
pub fn new(cli: &Cli, args: &NewArgs) -> Result<()> {
    let (catalog_path, mut catalog) = catalog::load_or_initialize(cli)?;
    let identity = normalize_identity(&args.source)?;
    if args.github && !identity.starts_with("github.com/") {
        bail!("--github requires a github.com/owner/repository identity");
    }
    if args.github {
        ensure_github_ready()?;
    }
    if existing_repo(&catalog, &identity).is_some() {
        bail!("repository is already catalogued: {identity}");
    }
    let target = absolute(&root_path(&catalog)?.join(&identity))?;
    if target.exists() {
        bail!(
            "canonical destination already exists: {}. Shu will not overwrite an existing directory",
            target.display()
        );
    }
    git::initialize(&target)?;
    if args.github {
        create_github_repository(&target, &identity, !args.public)?;
    }
    add_entry_with(
        &mut catalog,
        &identity,
        None,
        args.state,
        &args.tag,
        args.note.clone(),
    );
    let index = catalog.repos.len() - 1;
    remember_path(&mut catalog, index, &target)?;
    catalog::save(&catalog_path, &catalog)?;
    println!("Created {identity}");
    println!("  Local repository: {}", target.display());
    if args.github {
        println!("  GitHub repository: created");
    }
    Ok(())
}

/// Create the declared GitHub repository and attach it as `origin`.
pub(crate) fn create_github_repository(target: &Path, identity: &str, private: bool) -> Result<()> {
    let name = identity
        .strip_prefix("github.com/")
        .expect("GitHub identities were validated above");
    let mut command = std::process::Command::new("gh");
    command.args(["repo", "create", name]);
    command.arg(if private { "--private" } else { "--public" });
    let output = command
        .args(["--source"])
        .arg(target)
        .args(["--remote", "origin"])
        .output()
        .with_context(|| "could not run gh; install GitHub CLI or create the remote manually")?;
    if !output.status.success() {
        bail!(
            "could not create GitHub repository {name}: {}. Run `shu doctor --check-github` to verify gh access, or create the remote manually.",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Require an installed, authenticated GitHub CLI before changing the filesystem.
pub(crate) fn ensure_github_ready() -> Result<()> {
    let output = std::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
        .with_context(|| "could not run gh; install GitHub CLI or omit --github")?;
    if !output.status.success() {
        bail!(
            "gh is not authenticated: {}. Run `shu doctor --check-github` for details.",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Move a clean local working tree to its canonical Shu path, then catalog it.
fn migrate_and_add(cli: &Cli, args: &AddArgs, path: &Path, catalog: &mut Catalog) -> Result<()> {
    let source = migration_source(&args.source)?;
    let remote = git::output(&source, ["remote", "get-url", "origin"])?;
    let identity = normalize_identity(&remote)?;
    let destination = absolute(&root_path(catalog)?.join(&identity))?;
    let existing = existing_repo(catalog, &identity).is_some();

    println!("Migrate repository\n");
    println!("  Identity: {identity}");
    println!("  From:     {}", source.display());
    println!("  To:       {}", destination.display());

    if source == destination {
        println!("  {} Already at Shu's canonical path", ui::success_marker());
        if args.dry_run {
            println!("\nDry run: no files or catalog entries were changed.");
            return Ok(());
        }
        finish_catalog_add(
            path,
            catalog,
            args,
            &identity,
            ssh_transport(&remote),
            existing,
            false,
        )?;
        let index = existing_repo_index(catalog, &identity)
            .expect("catalog entry was added or already present");
        replace_path(catalog, index, &source, &destination)?;
        catalog::save(path, catalog)?;
        return Ok(());
    }
    if destination.starts_with(&source) {
        bail!(
            "canonical destination {} is inside the source repository {}; choose a repository root outside the source before migrating",
            destination.display(),
            source.display()
        );
    }
    if destination.exists() {
        bail!(
            "canonical destination already exists: {}. Shu will not overwrite an existing directory",
            destination.display()
        );
    }
    if !git::is_clean(&source)? {
        bail!(
            "working tree has staged, unstaged, or untracked changes: {}. Commit, stash, or remove the changes before migrating",
            source.display()
        );
    }
    println!("  {} Working tree is clean", ui::success_marker());
    if git::has_linked_worktrees(&source)? {
        bail!(
            "repository has linked Git worktrees. Shu will not move it because those worktrees may reference the current path"
        );
    }
    println!("  {} No linked worktrees", ui::success_marker());

    if args.dry_run {
        println!("\nDry run: no files or catalog entries were changed.");
        return Ok(());
    }
    if !confirm_migration(cli)? {
        println!("No changes made.");
        return Ok(());
    }
    if !same_filesystem(&source, &destination)? {
        bail!(
            "source and canonical destination are on different filesystems. Shu only performs atomic moves; choose a root on the same filesystem or move the repository manually"
        );
    }
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow!("canonical destination has no parent directory"))?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "could not create destination directory {}",
            parent.display()
        )
    })?;
    fs::rename(&source, &destination).with_context(|| {
        format!(
            "could not move {} to {}. Shu only performs atomic moves on the same filesystem; no repository files were copied or deleted",
            source.display(),
            destination.display()
        )
    })?;
    println!("  {} Moved repository", ui::success_marker());
    finish_catalog_add(
        path,
        catalog,
        args,
        &identity,
        ssh_transport(&remote),
        existing,
        true,
    )?;
    let index = existing_repo_index(catalog, &identity)
        .expect("catalog entry was added or already present");
    replace_path(catalog, index, &source, &destination)?;
    catalog::save(path, catalog)
}

/// Return whether the source and destination reside on a filesystem that supports rename.
fn same_filesystem(source: &Path, destination: &Path) -> Result<bool> {
    let destination_parent = destination
        .parent()
        .ok_or_else(|| anyhow!("canonical destination has no parent directory"))?;
    let existing_destination = destination_parent
        .ancestors()
        .find(|path| path.exists())
        .ok_or_else(|| anyhow!("canonical destination has no existing ancestor"))?;
    let existing_destination = fs::canonicalize(existing_destination).with_context(|| {
        format!(
            "could not resolve destination filesystem root {}",
            existing_destination.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(fs::metadata(source)?.dev() == fs::metadata(existing_destination)?.dev())
    }
    #[cfg(windows)]
    {
        let source_volume = volume_prefix(source)?;
        let destination_volume = volume_prefix(&existing_destination)?;
        Ok(source_volume.eq_ignore_ascii_case(&destination_volume))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (source, existing_destination);
        Ok(true)
    }
}

/// Return a Windows drive or UNC prefix for comparing filesystem roots.
#[cfg(windows)]
fn volume_prefix(path: &Path) -> Result<String> {
    use std::path::Component;

    match path.components().next() {
        Some(Component::Prefix(prefix)) => Ok(prefix
            .as_os_str()
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_string()),
        _ => bail!("could not determine filesystem root for {}", path.display()),
    }
}

/// Resolve `.` or an existing path to the root of a local Git working tree.
fn migration_source(value: &str) -> Result<PathBuf> {
    let requested = if value == "." {
        std::env::current_dir()?
    } else {
        PathBuf::from(value)
    };
    if !requested.exists() {
        bail!("--migrate requires `.` or an existing local Git working tree, not {value}");
    }
    git::worktree_root(&requested)
}

/// Return a working-tree root when an add argument refers to a local clone.
///
/// A normal `shu add .` records this path in the repository's `paths` list in
/// `shu.toml`. `--migrate` remains the explicit operation that moves a clone
/// into Shu's managed root.
fn local_source_path(value: &str) -> Result<Option<PathBuf>> {
    if value == "." || Path::new(value).exists() {
        return git::worktree_root(&if value == "." {
            std::env::current_dir()?
        } else {
            PathBuf::from(value)
        })
        .map(Some);
    }
    Ok(None)
}

/// Ask for explicit approval before a filesystem-moving migration.
fn confirm_migration(cli: &Cli) -> Result<bool> {
    if cli.yes {
        return Ok(true);
    }
    if cli.non_interactive {
        bail!("migration needs confirmation; rerun with --yes to approve it non-interactively");
    }
    eprint!("\nMove this repository into Shu's managed library? [y/N] ");
    io::stderr().flush()?;
    let mut response = String::new();
    io::stdin().read_line(&mut response)?;
    Ok(response.trim().eq_ignore_ascii_case("y") || response.trim().eq_ignore_ascii_case("yes"))
}

/// Finish catalog changes after migration or after confirming a canonical location.
fn finish_catalog_add(
    path: &Path,
    catalog: &mut Catalog,
    args: &AddArgs,
    identity: &str,
    remote: Option<String>,
    existing: bool,
    moved: bool,
) -> Result<()> {
    if existing {
        let index = existing_repo_index(catalog, identity)
            .expect("existing repository was checked before migration");
        if catalog.repos[index].remote.is_none() {
            catalog.repos[index].remote = remote;
        }
        println!(
            "  {} Preserved existing catalog metadata",
            ui::success_marker()
        );
    } else {
        add_entry(catalog, args, identity, remote);
        catalog::save(path, catalog)?;
        println!("  {} Added to catalog", ui::success_marker());
    }
    if moved {
        println!("\nMigrated {identity} into Shu's managed library.");
    }
    Ok(())
}

/// Return an existing catalog entry for one normalized identity, when present.
fn existing_repo<'a>(catalog: &'a Catalog, identity: &str) -> Option<&'a Repo> {
    catalog
        .repos
        .iter()
        .find(|repo| normalize_identity(&repo.source).ok().as_deref() == Some(identity))
}

/// Return the index of an existing entry for one normalized identity.
fn existing_repo_index(catalog: &Catalog, identity: &str) -> Option<usize> {
    catalog
        .repos
        .iter()
        .position(|repo| normalize_identity(&repo.source).ok().as_deref() == Some(identity))
}

/// Add one catalog entry using the metadata supplied to `shu add`.
fn add_entry(catalog: &mut Catalog, args: &AddArgs, identity: &str, remote: Option<String>) {
    add_entry_with(
        catalog,
        identity,
        remote,
        args.state,
        &args.tag,
        args.note.clone(),
    );
}

/// Add a catalog entry from shared repository metadata.
fn add_entry_with(
    catalog: &mut Catalog,
    identity: &str,
    remote: Option<String>,
    state: Lifecycle,
    tags: &[String],
    note: Option<String>,
) {
    catalog.repos.push(Repo {
        source: identity.to_owned(),
        remote,
        state,
        tags: catalog::unique(tags.to_vec()),
        note,
        paths: vec![],
        primary: None,
    });
}

/// Preserve only an explicit SSH transport; canonical identities use HTTPS.
fn ssh_transport(source: &str) -> Option<String> {
    let source = source.trim();
    (source.starts_with("git@") || source.starts_with("ssh://")).then(|| source.to_owned())
}

/// Remember a clone path and choose it only when the current primary is invalid.
fn remember_path(catalog: &mut Catalog, index: usize, path: &Path) -> Result<()> {
    let path = absolute(path)?;
    let replace_primary = locations::primary_path(catalog, &catalog.repos[index])?
        .is_none_or(|primary| !git::is_repo(&primary));
    let stored = locations::store_local_path(catalog, &path)?;
    let repo = &mut catalog.repos[index];
    if !repo.paths.iter().any(|known| known == &stored) {
        repo.paths.push(stored.clone());
    }
    if replace_primary {
        repo.primary = Some(stored);
    }
    Ok(())
}

/// Replace a moved source path with its canonical destination and prefer it.
fn replace_path(
    catalog: &mut Catalog,
    index: usize,
    source: &Path,
    destination: &Path,
) -> Result<()> {
    let source = locations::store_local_path(catalog, source)?;
    let destination = locations::store_local_path(catalog, destination)?;
    let repo = &mut catalog.repos[index];
    repo.paths.retain(|known| known != &source);
    if !repo.paths.iter().any(|known| known == &destination) {
        repo.paths.push(destination.clone());
    }
    repo.primary = Some(destination);
    Ok(())
}

/// Change the explicit lifecycle state or note for an existing catalog entry.
pub fn edit(cli: &Cli, args: &EditArgs) -> Result<()> {
    if args.state.is_none() && args.note.is_none() && !args.clear_note {
        bail!("nothing to edit; provide --state, --note, or --clear-note");
    }
    let (path, mut catalog) = catalog::load_or_initialize(cli)?;
    let index = catalog::select_index(&catalog, &args.selector)?;
    let repo = &mut catalog.repos[index];
    if let Some(state) = args.state {
        repo.state = state;
    }
    if let Some(note) = &args.note {
        repo.note = Some(note.clone());
    } else if args.clear_note {
        repo.note = None;
    }
    let name = catalog::repo_name(repo).to_owned();
    let state = repo.state;
    let note = repo.note.clone();
    catalog::save(&path, &catalog)?;

    println!("Updated {name}");
    println!("  State: {state}");
    match note {
        Some(note) => println!("  Note:  {note}"),
        None => println!("  Note:  none"),
    }
    Ok(())
}

/// Discover repositories below a directory and optionally import new identities.
pub fn scan(cli: &Cli, args: &ScanArgs) -> Result<()> {
    let found = discover_repos(&args.directory)?;
    if args.add {
        import_discovered(cli, &found)
    } else if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &found
                    .iter()
                    .map(|(path, identity, _)| serde_json::json!({"path": path, "identity": identity}))
                    .collect::<Vec<_>>()
            )?
        );
        Ok(())
    } else {
        for (path, identity, _) in found {
            let path = path.strip_prefix(&args.directory).unwrap_or(&path);
            print_scan_result(&identity, &path.display().to_string());
        }
        Ok(())
    }
}

/// Print one scan result as an identity/path record without terminal wrapping.
fn print_scan_result(identity: &str, path: &str) {
    let width = stdout_width();
    println!(
        "{}\n  {}",
        fit_terminal_width(identity, width),
        fit_terminal_width(path, width.saturating_sub(2))
    );
}

/// Preserve complete output for pipes while fitting interactive output to one line.
fn stdout_width() -> u16 {
    if io::stdout().is_terminal() {
        terminal::size().map_or(0, |(width, _)| width)
    } else {
        0
    }
}

/// Truncate a terminal line by characters, keeping an ellipsis inside its width.
fn fit_terminal_width(text: &str, width: u16) -> String {
    let width = usize::from(width);
    if width == 0 || text.chars().count() <= width {
        return text.to_owned();
    }
    if width == 1 {
        return "…".to_owned();
    }
    format!("{}…", text.chars().take(width - 1).collect::<String>())
}

/// Update a lifecycle field only; no repository files are touched.
pub fn change_state(cli: &Cli, selector: &str, state: Lifecycle) -> Result<()> {
    let (path, mut catalog) = catalog::load_or_initialize(cli)?;
    let index = catalog::select_index(&catalog, selector)?;
    catalog.repos[index].state = state;
    let name = catalog::repo_name(&catalog.repos[index]).to_owned();
    catalog::save(&path, &catalog)?;
    println!("Set {name} to {state}");
    Ok(())
}

/// Remove a catalog entry while leaving any local clone intact.
pub fn forget(cli: &Cli, args: &SelectorArgs) -> Result<()> {
    let (path, mut catalog) = catalog::load_or_initialize(cli)?;
    let index = catalog::select_index(&catalog, &args.selector)?;
    let removed = catalog.repos.remove(index);
    catalog::save(&path, &catalog)?;
    println!(
        "Removed {} from the catalog. Local repository was not deleted.",
        normalize_identity(&removed.source)?
    );
    Ok(())
}

/// Find repositories with an `origin` remote outside hidden directory trees.
pub(super) fn discover_repos(root: &Path) -> Result<Vec<(PathBuf, String, Option<String>)>> {
    if !root.exists() {
        bail!("scan directory does not exist: {}", root.display());
    }
    let mut found = vec![];
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(is_visible_scan_entry)
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_dir()
            || !entry.path().join(".git").exists()
            || git::is_shallow(entry.path())
            || git::is_submodule(entry.path())
        {
            continue;
        }
        if let Ok(remote) = git::output(entry.path(), ["remote", "get-url", "origin"])
            && let Ok(identity) = normalize_identity(&remote)
        {
            found.push((entry.path().to_path_buf(), identity, ssh_transport(&remote)));
        }
    }
    found.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
    Ok(found)
}

/// Keep build metadata and other hidden trees out of a predictable repository scan.
fn is_visible_scan_entry(entry: &DirEntry) -> bool {
    entry.depth() == 0 || !entry.file_name().to_string_lossy().starts_with('.')
}

/// Add only identities that are not already in the catalog.
fn import_discovered(cli: &Cli, found: &[(PathBuf, String, Option<String>)]) -> Result<()> {
    let (path, mut catalog) = catalog::load_or_initialize(cli)?;
    let mut known: HashSet<String> = catalog
        .repos
        .iter()
        .filter_map(|repo| normalize_identity(&repo.source).ok())
        .collect();
    let mut added = 0;
    for (local_path, identity, remote) in found {
        if known.insert(identity.clone()) {
            catalog.repos.push(Repo {
                source: identity.clone(),
                remote: remote.clone(),
                state: Lifecycle::Active,
                tags: vec![],
                note: None,
                paths: vec![],
                primary: None,
            });
            added += 1;
        }
        let index = existing_repo_index(&catalog, identity)
            .expect("discovered repository was added or already present");
        if catalog.repos[index].remote.is_none() {
            catalog.repos[index].remote = remote.clone();
        }
        remember_path(&mut catalog, index, local_path)?;
    }
    catalog::save(&path, &catalog)?;
    println!("Added {added} repository entries to {}", path.display());
    Ok(())
}
