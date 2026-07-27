//! Persistent catalog access, repository lookup, and application-state paths.

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

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

/// Return the machine-local repository-location sidecar for this catalog.
///
/// A custom catalog receives a sibling sidecar so test catalogs and portable
/// catalogs do not write into the user's normal Shu state directory.
pub fn local_state_path(cli: &Cli) -> Result<PathBuf> {
    if let Some(catalog) = &cli.catalog {
        return Ok(catalog.with_extension("local.json"));
    }
    Ok(state_dir()?.join("repositories.json"))
}

/// Look up a repository path observed on this machine, if one is recorded.
pub fn remembered_local_path(cli: &Cli, identity: &str) -> Result<Option<PathBuf>> {
    let identity = normalize_identity(identity)?;
    Ok(load_local_state(cli)?
        .repositories
        .get(&identity)
        .map(PathBuf::from))
}

/// Remember an existing local clone without adding machine-specific paths to
/// the portable catalog.
pub fn remember_local_path(cli: &Cli, identity: &str, path: &Path) -> Result<()> {
    let identity = normalize_identity(identity)?;
    let path = crate::paths::absolute(path)?;
    let mut state = load_local_state(cli)?;
    state
        .repositories
        .insert(identity, path.display().to_string());
    save_local_state(cli, &state)
}

/// Machine-local observations that are deliberately kept out of `shu.toml`.
#[derive(Debug, Deserialize, Serialize)]
struct LocalState {
    /// Format version for forward-compatible local cache changes.
    #[serde(default = "local_state_version")]
    version: u32,
    /// Last known working-tree path for each normalized repository identity.
    #[serde(default)]
    repositories: BTreeMap<String, String>,
}

impl Default for LocalState {
    fn default() -> Self {
        Self {
            version: local_state_version(),
            repositories: BTreeMap::new(),
        }
    }
}

/// Return the current local-state format version.
const fn local_state_version() -> u32 {
    1
}

/// Read the local observation sidecar, treating an absent file as empty state.
fn load_local_state(cli: &Cli) -> Result<LocalState> {
    let path = local_state_path(cli)?;
    let data = match fs::read(&path) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LocalState::default());
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("could not read local state {}", path.display()));
        }
    };
    let state: LocalState = serde_json::from_slice(&data)
        .with_context(|| format!("invalid local state {}", path.display()))?;
    if state.version != local_state_version() {
        bail!("unsupported local state version {}", state.version);
    }
    Ok(state)
}

/// Persist machine-local observations beside the selected catalog or state directory.
fn save_local_state(cli: &Cli, state: &LocalState) -> Result<()> {
    let path = local_state_path(cli)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("local state path has no parent directory"))?;
    fs::create_dir_all(parent)?;
    fs::write(&path, serde_json::to_vec_pretty(state)?)
        .with_context(|| format!("could not write local state {}", path.display()))
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

/// Load the active catalog, creating an empty one when it has not been set up yet.
///
/// This is used by everyday commands so a first run feels ready immediately.
/// Diagnostic commands keep using [`load`] so they can report a missing catalog
/// without changing the machine.
pub fn load_or_initialize(cli: &Cli) -> Result<(PathBuf, Catalog)> {
    let path = catalog_path(cli)?;
    if !path.exists() {
        save(
            &path,
            &Catalog {
                version: 1,
                root: crate::model::default_root(),
                repos: vec![],
            },
        )?;
    }
    load(cli)
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
            root: "~/shu".into(),
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
