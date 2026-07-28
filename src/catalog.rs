//! Persistent catalog access, repository lookup, and application-state paths.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    cli::{Cli, FilterArgs},
    git::git_output,
    identity::normalize_identity,
    model::{Catalog, Repo},
};
use anyhow::{Context, Result, anyhow, bail};
use directories::ProjectDirs;
use serde::Serialize;

/// Return the active catalog path, honoring the global `--catalog` override.
pub fn catalog_path(cli: &Cli) -> Result<PathBuf> {
    Ok(cli
        .catalog
        .clone()
        .unwrap_or_else(|| dirs().unwrap().config_dir().join("shu.toml")))
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
                sync: None,
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

/// Serialize only the catalog fields that are meaningful on every machine.
pub fn portable_contents(catalog: &Catalog) -> Result<String> {
    toml::to_string_pretty(&PortableCatalog::from(catalog)).map_err(Into::into)
}

/// Save the portable catalog projection to the dedicated sync checkout.
pub fn save_portable(path: &Path, catalog: &Catalog) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("catalog path has no parent"))?;
    fs::create_dir_all(parent)?;
    fs::write(path, portable_contents(catalog)?)
        .with_context(|| format!("could not write catalog {}", path.display()))
}

/// Merge remote repository metadata while retaining this machine's locations.
pub fn merge_portable(local: &mut Catalog, portable: Catalog) -> Result<()> {
    if portable.version != 1 {
        bail!("unsupported catalog version {}", portable.version);
    }
    let mut local_repos = std::mem::take(&mut local.repos)
        .into_iter()
        .map(|repo| Ok((normalize_identity(&repo.source)?, repo)))
        .collect::<Result<std::collections::HashMap<_, _>>>()?;
    local.repos = portable
        .repos
        .into_iter()
        .map(|mut repo| {
            if let Some(previous) = local_repos.remove(&normalize_identity(&repo.source)?) {
                repo.paths = previous.paths;
                repo.primary = previous.primary;
            }
            Ok(repo)
        })
        .collect::<Result<Vec<_>>>()?;
    local.sync = portable.sync;
    Ok(())
}

/// The schema written to the synced Git catalog.
#[derive(Serialize)]
struct PortableCatalog<'a> {
    version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    sync: Option<&'a crate::model::Sync>,
    repos: Vec<PortableRepo<'a>>,
}

impl<'a> From<&'a Catalog> for PortableCatalog<'a> {
    fn from(catalog: &'a Catalog) -> Self {
        Self {
            version: catalog.version,
            sync: catalog.sync.as_ref(),
            repos: catalog.repos.iter().map(PortableRepo::from).collect(),
        }
    }
}

/// Portable metadata for one repository.
#[derive(Serialize)]
struct PortableRepo<'a> {
    source: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote: Option<&'a String>,
    state: crate::model::Lifecycle,
    tags: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<&'a String>,
}

impl<'a> From<&'a Repo> for PortableRepo<'a> {
    fn from(repo: &'a Repo) -> Self {
        Self {
            source: &repo.source,
            remote: repo.remote.as_ref(),
            state: repo.state,
            tags: &repo.tags,
            note: repo.note.as_ref(),
        }
    }
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
    ProjectDirs::from("com", "wiedymi", "shu")
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
                    remote: None,
                    state: Lifecycle::Active,
                    tags: vec![],
                    note: None,
                    paths: vec![],
                    primary: None,
                },
                Repo {
                    source: "github.com/acme/api".into(),
                    remote: None,
                    state: Lifecycle::Parked,
                    tags: vec![],
                    note: None,
                    paths: vec![],
                    primary: None,
                },
            ],
            sync: None,
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

    #[cfg(target_os = "macos")]
    #[test]
    fn uses_the_com_wiedymi_shu_application_identifier() {
        assert!(
            dirs()
                .unwrap()
                .config_dir()
                .to_string_lossy()
                .contains("com.wiedymi.shu")
        );
    }
}
