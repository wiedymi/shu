//! Commands that create, inspect, and edit catalog entries.

use std::{
    collections::HashSet,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use walkdir::WalkDir;

use crate::{
    catalog::{self, catalog_path},
    cli::{AddArgs, Cli, EditArgs, ScanArgs, SelectorArgs},
    git,
    identity::normalize_identity,
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
        },
    )?;
    println!("Initialized Shu catalog at {}", path.display());
    Ok(())
}

/// Add an identity or the current Git repository to the active catalog.
pub fn add(cli: &Cli, args: &AddArgs) -> Result<()> {
    let (path, mut catalog) = catalog::load_or_initialize(cli)?;
    if args.migrate {
        return migrate_and_add(cli, args, &path, &mut catalog);
    }
    let identity = normalize_identity(&catalog::source_from_argument(&args.source)?)?;
    let local_path = local_source_path(&args.source)?;
    if let Some(existing) = existing_repo(&catalog, &identity) {
        if let Some(local_path) = local_path {
            catalog::remember_local_path(cli, &identity, &local_path)?;
            println!("Already catalogued {}", catalog::repo_name(existing));
            println!("  Local clone: {}", local_path.display());
            return Ok(());
        }
        bail!(
            "{identity} is already in the catalog. To change its state or note, run `shu edit {} --state <state> --note <text>`",
            catalog::repo_name(existing)
        );
    }
    add_entry(&mut catalog, args, &identity);
    catalog::save(&path, &catalog)?;
    println!("Added {identity}");
    if let Some(local_path) = local_path {
        catalog::remember_local_path(cli, &identity, &local_path)?;
        println!("  Local clone: {}", local_path.display());
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
        finish_catalog_add(path, catalog, args, &identity, existing, false)?;
        catalog::remember_local_path(cli, &identity, &destination)?;
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
    finish_catalog_add(path, catalog, args, &identity, existing, true)?;
    catalog::remember_local_path(cli, &identity, &destination)
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
/// A normal `shu add .` records this path in machine-local state. The portable
/// catalog only keeps repository identity, leaving `--migrate` as the explicit
/// operation that moves a clone into Shu's managed root.
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
    existing: bool,
    moved: bool,
) -> Result<()> {
    if existing {
        println!(
            "  {} Preserved existing catalog metadata",
            ui::success_marker()
        );
    } else {
        add_entry(catalog, args, identity);
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

/// Add one catalog entry using the metadata supplied to `shu add`.
fn add_entry(catalog: &mut Catalog, args: &AddArgs, identity: &str) {
    catalog.repos.push(Repo {
        source: identity.to_owned(),
        state: args.state,
        tags: catalog::unique(args.tag.clone()),
        note: args.note.clone(),
    });
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
                    .map(|(path, identity)| serde_json::json!({"path": path, "identity": identity}))
                    .collect::<Vec<_>>()
            )?
        );
        Ok(())
    } else {
        for (path, identity) in found {
            println!("{identity}\t{}", path.display());
        }
        Ok(())
    }
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

/// Find repositories with an `origin` remote, ignoring invalid directories.
pub(super) fn discover_repos(root: &Path) -> Result<Vec<(PathBuf, String)>> {
    if !root.exists() {
        bail!("scan directory does not exist: {}", root.display());
    }
    let mut found = vec![];
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != ".git")
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_dir() || !entry.path().join(".git").exists() {
            continue;
        }
        if let Ok(remote) = git::output(entry.path(), ["remote", "get-url", "origin"])
            && let Ok(identity) = normalize_identity(&remote)
        {
            found.push((entry.path().to_path_buf(), identity));
        }
    }
    found.sort_by(|left, right| left.1.cmp(&right.1));
    Ok(found)
}

/// Add only identities that are not already in the catalog.
fn import_discovered(cli: &Cli, found: &[(PathBuf, String)]) -> Result<()> {
    let (path, mut catalog) = catalog::load_or_initialize(cli)?;
    let mut known: HashSet<String> = catalog
        .repos
        .iter()
        .filter_map(|repo| normalize_identity(&repo.source).ok())
        .collect();
    let mut added = 0;
    for (local_path, identity) in found {
        if known.insert(identity.clone()) {
            catalog.repos.push(Repo {
                source: identity.clone(),
                state: Lifecycle::Active,
                tags: vec![],
                note: None,
            });
            added += 1;
        }
        catalog::remember_local_path(cli, identity, local_path)?;
    }
    catalog::save(&path, &catalog)?;
    println!("Added {added} repository entries to {}", path.display());
    Ok(())
}
