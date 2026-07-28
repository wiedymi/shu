//! Read-only status and listing commands.

use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, bail};

use crate::{
    catalog::{self, catalog_path, repo_name},
    cli::{Cli, FilterArgs, ListArgs, LocationsArgs},
    identity::normalize_identity,
    locations,
    model::{Catalog, ListOutput, Repo, RepoOutput},
    paths::{absolute, root_path},
    ui,
};

use super::catalog::discover_repos;

/// Compare declared catalog state against repositories detected locally.
pub fn status(cli: &Cli, filter: &FilterArgs) -> Result<()> {
    let (_, catalog) = catalog::load_or_initialize(cli)?;
    let repos = catalog::filtered(&catalog, filter)?;
    if cli.json {
        return print_json(cli, &catalog, repos);
    }
    ui::heading("Library status");
    ui::detail("catalog", catalog_path(cli)?.display());
    ui::detail("root", root_path(&catalog)?.display());
    println!();
    print_grouped_status(&catalog, repos)?;
    print_uncatalogued(&catalog)
}

/// List filtered entries in a compact one-line format or as JSON.
pub fn list(cli: &Cli, args: &ListArgs) -> Result<()> {
    let (_, catalog) = catalog::load_or_initialize(cli)?;
    let max_age = args.stale.as_deref().map(parse_duration).transpose()?;
    let repos = catalog::filtered(&catalog, &args.filter)?
        .into_iter()
        .filter(|repo| {
            let path = locations::present_path(&catalog, repo).ok().flatten();
            (!args.missing || path.is_none())
                && max_age.is_none_or(|age| path.is_some_and(|path| stale(&path, age)))
        })
        .collect::<Vec<_>>();
    if cli.json {
        return print_json(cli, &catalog, repos);
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
    if let Some(path) = locations::present_path(catalog, repo)? {
        return Ok(if stale(&path, Duration::from_secs(180 * 86400)) {
            "present (stale candidate)".into()
        } else {
            "present".into()
        });
    }
    let remembered_exists = locations::remembered_paths(catalog, repo)?
        .iter()
        .any(|path| path.exists());
    let managed = locations::managed_path(catalog, repo)?;
    Ok(if remembered_exists || managed.exists() {
        "invalid".into()
    } else {
        "missing".into()
    })
}

/// Emit the stable JSON contract shared by status and list.
fn print_json(_cli: &Cli, catalog: &Catalog, repos: Vec<&Repo>) -> Result<()> {
    let repositories = repos
        .into_iter()
        .map(|repo| {
            let path = locations::present_path(catalog, repo)?
                .unwrap_or(locations::managed_path(catalog, repo)?);
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

/// Print lifecycle sections and catalogued clone paths for each repository.
fn print_grouped_status(catalog: &Catalog, repos: Vec<&Repo>) -> Result<()> {
    if repos.is_empty() {
        println!("No repositories are catalogued yet.");
        println!("  Add the current repository:  shu add .");
        println!("  Restore a saved library:     shu restore <source>");
        return Ok(());
    }

    let mut current = None;
    for repo in repos {
        if current != Some(repo.state) {
            current = Some(repo.state);
            println!("{}", ui::accent(repo.state.to_string().to_uppercase()));
        }
        print_repository_status(catalog, repo)?;
    }
    ui::detail(
        "next",
        "shu edit <repository> --state <state> --note <text>",
    );
    Ok(())
}

/// Print one status entry with the expected path and next action when missing.
fn print_repository_status(catalog: &Catalog, repo: &Repo) -> Result<()> {
    let observed = observed_state(catalog, repo)?;
    let marker = match observed.as_str() {
        value if value.starts_with("present") => ui::success_marker(),
        "missing" => ui::warning_marker(),
        _ => ui::failure_marker(),
    };
    println!("  {marker} {:<18} {observed}", repo_name(repo));

    if observed == "missing" {
        for path in locations::remembered_paths(catalog, repo)? {
            println!("    Recorded: {}", path.display());
        }
        let path = locations::managed_path(catalog, repo)?;
        println!("    Expected: {}", path.display());
        println!("    Clone:    shu ensure {}", repo_name(repo));
    } else {
        let primary = locations::present_path(catalog, repo)?;
        let paths = locations::present_paths(catalog, repo)?;
        let managed = locations::managed_path(catalog, repo)?;
        if paths.len() > 1 || paths.first().is_some_and(|path| path != &managed) {
            println!("    Clones:");
            for path in paths {
                let marker = if primary.as_ref() == Some(&path) {
                    "*"
                } else {
                    "·"
                };
                println!("      {marker} {}", path.display());
            }
        }
    }
    if let Some(note) = &repo.note {
        println!("    Note:     {note}");
    }
    if !repo.tags.is_empty() {
        println!("    Tags:     {}", repo.tags.join(", "));
    }
    Ok(())
}

/// Show known clone paths or select the preferred clone for one repository.
pub fn locations_command(cli: &Cli, args: &LocationsArgs) -> Result<()> {
    let (catalog_path, mut catalog) = catalog::load_or_initialize(cli)?;
    let index = catalog::select_index(&catalog, &args.selector)?;
    if let Some(path) = &args.primary {
        let path = absolute(path)?;
        let path = if crate::git::is_repo(&path) {
            crate::git::worktree_root(&path)?
        } else {
            path
        };
        let name = catalog::repo_name(&catalog.repos[index]).to_owned();
        let stored = locations::store_local_path(&catalog, &path)?;
        let is_recorded = catalog.repos[index]
            .paths
            .iter()
            .any(|known| known == &stored);
        let is_managed = path == locations::managed_path(&catalog, &catalog.repos[index])?
            && crate::git::is_repo(&path);
        if !is_recorded && !is_managed {
            bail!(
                "{} is not a known clone for {}; run `shu add .` from that clone first",
                path.display(),
                name
            );
        }
        if !is_recorded {
            catalog.repos[index].paths.push(stored.clone());
        }
        catalog.repos[index].primary = Some(stored);
        catalog::save(&catalog_path, &catalog)?;
        println!("Preferred clone for {name}: {}", path.display());
    }

    let repo = &catalog.repos[index];
    let primary = locations::present_path(&catalog, repo)?;
    let mut paths = locations::remembered_paths(&catalog, repo)?;
    let managed = locations::managed_path(&catalog, repo)?;
    if crate::git::is_repo(&managed) && !paths.iter().any(|path| path == &managed) {
        paths.push(managed);
    }
    if paths.is_empty() {
        println!("No clone paths are recorded for {}.", repo_name(repo));
        println!("  Add an existing clone: shu add .");
        println!("  Create the managed clone: shu ensure {}", repo_name(repo));
        return Ok(());
    }
    println!("{}", repo_name(repo));
    for path in paths {
        let marker = if primary.as_ref() == Some(&path) {
            "*"
        } else {
            "·"
        };
        let state = if crate::git::is_repo(&path) {
            "present"
        } else {
            "missing"
        };
        println!("  {marker} {state:<7} {}", path.display());
    }
    let worktrees = locations::pickable_paths(&catalog, repo)?;
    let clone_paths = locations::present_paths(&catalog, repo)?;
    let linked = worktrees
        .into_iter()
        .filter(|path| !clone_paths.contains(path))
        .collect::<Vec<_>>();
    if !linked.is_empty() {
        println!("  Worktrees:");
        for path in linked {
            println!("    · {}", path.display());
        }
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
        .filter(|(_, identity, _)| {
            !catalog
                .repos
                .iter()
                .any(|repo| normalize_identity(&repo.source).ok().as_deref() == Some(identity))
        })
        .collect::<Vec<_>>();
    if !uncatalogued.is_empty() {
        println!("\nUNCATALOGUED");
        for (path, identity, _) in uncatalogued {
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
