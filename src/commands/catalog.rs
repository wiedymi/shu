//! Commands that create, inspect, and edit catalog entries.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use walkdir::WalkDir;

use crate::{
    catalog::{self, catalog_path},
    cli::{AddArgs, Cli, EditArgs, ScanArgs, SelectorArgs},
    git,
    identity::normalize_identity,
    model::{Catalog, Lifecycle, Repo},
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
    let identity = normalize_identity(&catalog::source_from_argument(&args.source)?)?;
    if let Some(existing) = catalog
        .repos
        .iter()
        .find(|repo| normalize_identity(&repo.source).ok().as_deref() == Some(&identity))
    {
        bail!(
            "{identity} is already in the catalog. To change its state or note, run `shu edit {} --state <state> --note <text>`",
            catalog::repo_name(existing)
        );
    }
    catalog.repos.push(Repo {
        source: identity.clone(),
        state: args.state,
        tags: catalog::unique(args.tag.clone()),
        note: args.note.clone(),
    });
    catalog::save(&path, &catalog)?;
    println!("Added {identity}");
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
    for (_, identity) in found {
        if known.insert(identity.clone()) {
            catalog.repos.push(Repo {
                source: identity.clone(),
                state: Lifecycle::Active,
                tags: vec![],
                note: None,
            });
            added += 1;
        }
    }
    catalog::save(&path, &catalog)?;
    println!("Added {added} repository entries to {}", path.display());
    Ok(())
}
