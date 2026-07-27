//! Commands that materialize catalog entries as local repositories.

use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

use anyhow::{Context, Result, bail};

use crate::{
    catalog::{self, catalog_path, repo_name},
    cli::{Cli, EnsureArgs, FilterArgs, RestoreArgs, SelectorArgs, UpdateArgs},
    git, locations,
    model::{Catalog, Origin},
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

/// Refresh the remembered remote source, then restore the refreshed catalog.
pub fn update(cli: &Cli, args: &UpdateArgs) -> Result<()> {
    if args.selector.is_some() || args.state.is_some() || args.note.is_some() {
        let selector = args.selector.as_deref().unwrap_or("<repository>");
        bail!(
            "`shu update` refreshes the saved catalog source; it does not edit a repository. Use `shu edit {selector} --state <state> --note <text>` instead"
        );
    }
    let origin: Origin = serde_json::from_slice(
        &fs::read(catalog::origin_path(cli)?)
            .context("no saved catalog source; run `shu restore <source>` first")?,
    )?;
    restore(
        cli,
        &RestoreArgs {
            source: Some(origin.source),
            file: origin.file.map(PathBuf::from),
            git_ref: origin.git_ref,
            filter: FilterArgs::default(),
        },
    )
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

/// Resolve, validate, and persist a remote catalog source before restoration.
fn activate_source(cli: &Cli, args: &RestoreArgs, source: &str) -> Result<()> {
    let content = sources::resolve(source, args.file.as_deref(), args.git_ref.as_deref())?;
    let loaded: Catalog = toml::from_str(&content).context("resolved catalog is not valid TOML")?;
    if loaded.version != 1 {
        bail!("unsupported catalog version {}", loaded.version);
    }
    catalog::save(&catalog_path(cli)?, &loaded)?;
    let origin = Origin {
        source: source.to_owned(),
        file: args.file.as_ref().map(|file| file.display().to_string()),
        git_ref: args.git_ref.clone(),
    };
    let path = catalog::origin_path(cli)?;
    fs::create_dir_all(path.parent().expect("origin path always has a parent"))?;
    fs::write(path, serde_json::to_vec_pretty(&origin)?)?;
    eprintln!("Using catalog from {source}");
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
