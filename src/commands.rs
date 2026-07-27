//! User-facing command implementations.
//!
//! This module coordinates catalog, filesystem, Git, and source-resolution
//! layers while leaving their low-level behavior in dedicated modules.

use std::{
    collections::HashSet,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, bail};
use walkdir::WalkDir;

use crate::{
    catalog::{self, catalog_path, repo_name},
    cli::{AddArgs, Cli, EnsureArgs, FilterArgs, ListArgs, RestoreArgs, ScanArgs, SelectorArgs},
    git,
    identity::normalize_identity,
    model::{Catalog, Lifecycle, ListOutput, Origin, Repo, RepoOutput},
    paths::{absolute, repo_path, root_path},
    sources,
};

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

pub fn add(cli: &Cli, args: &AddArgs) -> Result<()> {
    let (path, mut catalog) = catalog::load(cli)?;
    let identity = normalize_identity(&catalog::source_from_argument(&args.source)?)?;
    if catalog
        .repos
        .iter()
        .any(|repo| normalize_identity(&repo.source).ok().as_deref() == Some(&identity))
    {
        bail!("{identity} is already in the catalog");
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

pub fn scan(cli: &Cli, args: &ScanArgs) -> Result<()> {
    let found = discover_repos(&args.directory)?;
    if args.add {
        let (path, mut catalog) = catalog::load(cli)?;
        let mut known: HashSet<String> = catalog
            .repos
            .iter()
            .filter_map(|repo| normalize_identity(&repo.source).ok())
            .collect();
        let mut added = 0;
        for (_, identity) in &found {
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
    } else {
        for (path, identity) in found {
            println!("{identity}\t{}", path.display());
        }
    }
    Ok(())
}

pub fn status(cli: &Cli, filter: &FilterArgs) -> Result<()> {
    let (_, catalog) = catalog::load(cli)?;
    let repos = catalog::filtered(&catalog, filter).collect::<Vec<_>>();
    if cli.json {
        return print_json(&catalog, repos);
    }
    println!("Catalog: {}", catalog_path(cli)?.display());
    println!("Root:    {}\n", root_path(&catalog)?.display());
    let mut current = None;
    for repo in repos {
        if current != Some(repo.state) {
            current = Some(repo.state);
            println!("{}", repo.state.to_string().to_uppercase());
        }
        println!(
            "  {:<18} {}",
            repo_name(repo),
            observed_state(&catalog, repo)?
        );
    }
    let root = root_path(&catalog)?;
    let uncatalogued = if root.exists() {
        discover_repos(&root)?
            .into_iter()
            .filter(|(_, identity)| {
                !catalog
                    .repos
                    .iter()
                    .any(|repo| normalize_identity(&repo.source).ok().as_deref() == Some(identity))
            })
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    if !uncatalogued.is_empty() {
        println!("\nUNCATALOGUED");
        for (path, identity) in uncatalogued {
            println!("  {:<18} {}", identity, path.display());
        }
    }
    Ok(())
}

pub fn restore(cli: &Cli, args: &RestoreArgs) -> Result<()> {
    if let Some(source) = &args.source {
        let content = sources::resolve(source, args.file.as_deref(), args.git_ref.as_deref())?;
        let loaded: Catalog =
            toml::from_str(&content).context("resolved catalog is not valid TOML")?;
        if loaded.version != 1 {
            bail!("unsupported catalog version {}", loaded.version);
        }
        let target = catalog_path(cli)?;
        catalog::save(&target, &loaded)?;
        let origin = Origin {
            source: source.clone(),
            file: args.file.as_ref().map(|file| file.display().to_string()),
            git_ref: args.git_ref.clone(),
        };
        let origin_file = catalog::origin_path(cli)?;
        fs::create_dir_all(origin_file.parent().unwrap())?;
        fs::write(origin_file, serde_json::to_vec_pretty(&origin)?)?;
        eprintln!("Using catalog from {source}");
    }
    let (_, catalog) = catalog::load(cli)?;
    let repos = catalog::filtered(&catalog, &args.filter).collect::<Vec<_>>();
    if args.source.is_some() && !cli.yes && !cli.non_interactive {
        eprint!(
            "Catalog contains {} repositories. Continue? [Y/n] ",
            repos.len()
        );
        io::stderr().flush()?;
        let mut response = String::new();
        io::stdin().read_line(&mut response)?;
        if !response.trim().is_empty()
            && !response.trim().eq_ignore_ascii_case("y")
            && !response.trim().eq_ignore_ascii_case("yes")
        {
            println!("No changes made.");
            return Ok(());
        }
    }
    fs::create_dir_all(root_path(&catalog)?)?;
    let mut failures = 0;
    for repo in repos {
        let target = repo_path(&catalog, repo)?;
        if git::is_repo(&target) {
            println!("✓ {} already present", repo_name(repo));
        } else if target.exists() {
            eprintln!(
                "! {} exists but is not a valid Git repository: {}",
                repo_name(repo),
                target.display()
            );
            failures += 1;
        } else {
            eprintln!("↓ cloning {}", repo.source);
            if let Err(error) = git::clone(&repo.source, &target) {
                eprintln!("! {}: {error:#}", repo_name(repo));
                failures += 1;
            }
        }
    }
    if failures > 0 {
        bail!("restore completed with {failures} conflict(s) or clone failure(s)");
    }
    Ok(())
}

pub fn update(cli: &Cli) -> Result<()> {
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

pub fn ensure(cli: &Cli, args: &EnsureArgs) -> Result<()> {
    let (_, catalog) = catalog::load(cli)?;
    let repo = catalog::select(&catalog, &args.selector)?;
    let target = repo_path(&catalog, repo)?;
    if !git::is_repo(&target) {
        if target.exists() {
            bail!(
                "target path exists but is not a valid Git repository: {}",
                target.display()
            );
        }
        eprintln!("↓ cloning {}", repo.source);
        git::clone(&repo.source, &target)?;
    }
    let target = absolute(&target)?;
    if args.path_only {
        println!("{}", target.display());
    } else {
        println!("Ensured {} at {}", repo_name(repo), target.display());
    }
    Ok(())
}

pub fn path_command(cli: &Cli, args: &SelectorArgs) -> Result<()> {
    let (_, catalog) = catalog::load(cli)?;
    let repo = catalog::select(&catalog, &args.selector)?;
    let target = repo_path(&catalog, repo)?;
    if !git::is_repo(&target) {
        bail!("repository is missing locally: {}", target.display());
    }
    println!("{}", absolute(&target)?.display());
    Ok(())
}

pub fn list(cli: &Cli, args: &ListArgs) -> Result<()> {
    let (_, catalog) = catalog::load(cli)?;
    let max_age = args.stale.as_deref().map(parse_duration).transpose()?;
    let repos = catalog::filtered(&catalog, &args.filter)
        .filter(|repo| {
            let path = repo_path(&catalog, repo).ok();
            (!args.missing || path.as_ref().is_some_and(|path| !git::is_repo(path)))
                && max_age.is_none_or(|age| path.is_some_and(|path| stale(&path, age)))
        })
        .collect::<Vec<_>>();
    if cli.json {
        return print_json(&catalog, repos);
    }
    for repo in repos {
        println!(
            "{:<24} {:<10} {}",
            normalize_identity(&repo.source)?,
            repo.state,
            observed_state(&catalog, repo)?
        );
    }
    Ok(())
}

pub fn change_state(cli: &Cli, selector: &str, state: Lifecycle) -> Result<()> {
    let (path, mut catalog) = catalog::load(cli)?;
    let index = catalog::select_index(&catalog, selector)?;
    catalog.repos[index].state = state;
    let name = repo_name(&catalog.repos[index]).to_owned();
    catalog::save(&path, &catalog)?;
    println!("Set {name} to {state}");
    Ok(())
}

pub fn forget(cli: &Cli, args: &SelectorArgs) -> Result<()> {
    let (path, mut catalog) = catalog::load(cli)?;
    let index = catalog::select_index(&catalog, &args.selector)?;
    let removed = catalog.repos.remove(index);
    catalog::save(&path, &catalog)?;
    println!(
        "Removed {} from the catalog. Local repository was not deleted.",
        normalize_identity(&removed.source)?
    );
    Ok(())
}

fn discover_repos(root: &Path) -> Result<Vec<(PathBuf, String)>> {
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

fn observed_state(catalog: &Catalog, repo: &Repo) -> Result<String> {
    let path = repo_path(catalog, repo)?;
    Ok(if git::is_repo(&path) {
        if stale(&path, Duration::from_secs(180 * 86400)) {
            "present (stale candidate)".into()
        } else {
            "present".into()
        }
    } else if path.exists() {
        "invalid".into()
    } else {
        "missing".into()
    })
}

fn print_json(catalog: &Catalog, repos: Vec<&Repo>) -> Result<()> {
    let repositories = repos
        .into_iter()
        .map(|repo| {
            let path = repo_path(catalog, repo)?;
            Ok(RepoOutput {
                identity: normalize_identity(&repo.source)?,
                name: repo_name(repo).to_owned(),
                path: absolute(&path)?.display().to_string(),
                declared_state: repo.state,
                observed_state: observed_state(catalog, repo)?,
                tags: repo.tags.clone(),
                note: repo.note.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&ListOutput {
            schema_version: 1,
            repositories
        })?
    );
    Ok(())
}

fn stale(path: &Path, max_age: Duration) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age > max_age)
}
fn parse_duration(value: &str) -> Result<Duration> {
    Ok(Duration::from_secs(
        value
            .trim_end_matches('d')
            .parse::<u64>()
            .context("stale duration must be like 180d")?
            * 86400,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_stale_duration() {
        assert_eq!(
            parse_duration("180d").unwrap(),
            Duration::from_secs(180 * 86400)
        );
    }
}
