//! Persistent catalog access, repository lookup, and application-state paths.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use directories::ProjectDirs;

use crate::{
    cli::{Cli, FilterArgs},
    git::git_output,
    identity::normalize_identity,
    model::{Catalog, Repo},
};

/// Return the active catalog path, honoring the global `--catalog` override.
pub fn catalog_path(cli: &Cli) -> Result<PathBuf> {
    Ok(cli
        .catalog
        .clone()
        .unwrap_or_else(|| dirs().unwrap().config_dir().join("shu.toml")))
}

/// Return the sidecar file that records the source used by `shu update`.
pub fn origin_path(cli: &Cli) -> Result<PathBuf> {
    if let Some(catalog) = &cli.catalog {
        return Ok(catalog.with_extension("origin.json"));
    }
    Ok(dirs()?.config_dir().join("origin.json"))
}

/// Return Shu's disposable, machine-local state directory.
pub fn state_dir() -> Result<PathBuf> {
    let project = dirs()?;
    Ok(project
        .state_dir()
        .unwrap_or_else(|| project.data_local_dir())
        .to_path_buf())
}

/// Load and validate the selected catalog, returning its path and parsed data.
pub fn load(cli: &Cli) -> Result<(PathBuf, Catalog)> {
    let path = catalog_path(cli)?;
    let data = fs::read_to_string(&path).with_context(|| {
        format!(
            "could not read catalog {}; run `shu init` first",
            path.display()
        )
    })?;
    let catalog: Catalog =
        toml::from_str(&data).with_context(|| format!("invalid catalog {}", path.display()))?;
    if catalog.version != 1 {
        bail!("unsupported catalog version {}", catalog.version);
    }
    Ok((path, catalog))
}

/// Serialize a catalog to TOML, creating its parent directory when necessary.
pub fn save(path: &Path, catalog: &Catalog) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("catalog path has no parent"))?;
    fs::create_dir_all(parent)?;
    fs::write(path, toml::to_string_pretty(catalog)?)
        .with_context(|| format!("could not write catalog {}", path.display()))
}

/// Resolve `.` or a local repository path to its origin remote; pass other values through.
pub fn source_from_argument(value: &str) -> Result<String> {
    if value == "." || Path::new(value).exists() {
        let path = if value == "." {
            std::env::current_dir()?
        } else {
            PathBuf::from(value)
        };
        return git_output(&path, ["remote", "get-url", "origin"]);
    }
    Ok(value.to_owned())
}

/// Iterate catalog entries that satisfy optional tag and lifecycle filters.
pub fn filtered<'a>(
    catalog: &'a Catalog,
    filter: &'a FilterArgs,
) -> impl Iterator<Item = &'a Repo> {
    catalog.repos.iter().filter(move |repo| {
        filter.state.is_none_or(|state| repo.state == state)
            && filter
                .tag
                .as_ref()
                .is_none_or(|tag| repo.tags.iter().any(|item| item == tag))
    })
}

/// Resolve a selector to exactly one catalog entry.
pub fn select<'a>(catalog: &'a Catalog, selector: &str) -> Result<&'a Repo> {
    Ok(&catalog.repos[select_index(catalog, selector)?])
}

/// Resolve a selector to an index, rejecting absent and ambiguous matches.
pub fn select_index(catalog: &Catalog, selector: &str) -> Result<usize> {
    let selector = if Path::new(selector).exists() {
        normalize_identity(&source_from_argument(selector)?)?
    } else {
        selector
            .trim()
            .trim_end_matches(".git")
            .trim_matches('/')
            .to_owned()
    };
    let matches = catalog
        .repos
        .iter()
        .enumerate()
        .filter_map(|(index, repo)| {
            let identity = normalize_identity(&repo.source).ok()?;
            (identity == selector
                || identity.ends_with(&format!("/{selector}"))
                || repo_name(repo) == selector)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => bail!("repository not found in catalog: {selector}"),
        _ => bail!("ambiguous repository selector: {selector}"),
    }
}

/// Return the final path component of a repository identity.
pub fn repo_name(repo: &Repo) -> &str {
    repo.source
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .next()
        .unwrap_or(&repo.source)
}

/// Remove duplicate strings while preserving their first-seen order.
pub fn unique(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(item.clone()))
        .collect()
}

/// Resolve platform-native application directories.
fn dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("dev", "wiedymi", "shu")
        .ok_or_else(|| anyhow!("could not determine Shu configuration directory"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Lifecycle;

    fn catalog() -> Catalog {
        Catalog {
            version: 1,
            root: "~/Code".into(),
            repos: vec![
                Repo {
                    source: "github.com/acme/widgets".into(),
                    state: Lifecycle::Active,
                    tags: vec![],
                    note: None,
                },
                Repo {
                    source: "github.com/acme/api".into(),
                    state: Lifecycle::Parked,
                    tags: vec![],
                    note: None,
                },
            ],
        }
    }

    #[test]
    fn resolves_a_unique_repository_name() {
        assert_eq!(
            select(&catalog(), "widgets").unwrap().source,
            "github.com/acme/widgets"
        );
    }

    #[test]
    fn removes_duplicate_tags_without_reordering() {
        assert_eq!(
            unique(vec!["work".into(), "rust".into(), "work".into()]),
            ["work", "rust"]
        );
    }
}
