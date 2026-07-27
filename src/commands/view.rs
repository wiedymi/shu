//! Read-only status and listing commands.

use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};

use crate::{
    catalog::{self, catalog_path, repo_name},
    cli::{Cli, FilterArgs, ListArgs},
    git,
    identity::normalize_identity,
    model::{Catalog, ListOutput, Repo, RepoOutput},
    paths::{absolute, repo_path, root_path},
};

use super::catalog::discover_repos;

/// Compare declared catalog state against repositories detected locally.
pub fn status(cli: &Cli, filter: &FilterArgs) -> Result<()> {
    let (_, catalog) = catalog::load(cli)?;
    let repos = catalog::filtered(&catalog, filter).collect::<Vec<_>>();
    if cli.json {
        return print_json(&catalog, repos);
    }
    println!("Catalog: {}", catalog_path(cli)?.display());
    println!("Root:    {}\n", root_path(&catalog)?.display());
    print_grouped_status(&catalog, repos)?;
    print_uncatalogued(&catalog)
}

/// List filtered entries in a compact one-line format or as JSON.
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

/// Describe a repository's local presence without changing it.
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

/// Emit the stable JSON contract shared by status and list.
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

/// Print lifecycle sections and local state for each selected repository.
fn print_grouped_status(catalog: &Catalog, repos: Vec<&Repo>) -> Result<()> {
    let mut current = None;
    for repo in repos {
        if current != Some(repo.state) {
            current = Some(repo.state);
            println!("{}", repo.state.to_string().to_uppercase());
        }
        println!(
            "  {:<18} {}",
            repo_name(repo),
            observed_state(catalog, repo)?
        );
    }
    Ok(())
}

/// Display repositories found under the root that are absent from the catalog.
fn print_uncatalogued(catalog: &Catalog) -> Result<()> {
    let root = root_path(catalog)?;
    if !root.exists() {
        return Ok(());
    }
    let uncatalogued = discover_repos(&root)?
        .into_iter()
        .filter(|(_, identity)| {
            !catalog
                .repos
                .iter()
                .any(|repo| normalize_identity(&repo.source).ok().as_deref() == Some(identity))
        })
        .collect::<Vec<_>>();
    if !uncatalogued.is_empty() {
        println!("\nUNCATALOGUED");
        for (path, identity) in uncatalogued {
            println!("  {:<18} {}", identity, path.display());
        }
    }
    Ok(())
}

/// Return whether filesystem activity is older than the supplied threshold.
fn stale(path: &Path, max_age: Duration) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age > max_age)
}

/// Parse a days-only duration such as `180d`.
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
