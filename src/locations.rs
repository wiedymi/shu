//! Resolution of a repository's observed and managed local locations.
//!
//! `shu.toml` remains portable and therefore contains no machine-specific
//! paths. This module combines its canonical destination with the local
//! observation sidecar written by commands such as `shu add .`.

use std::path::PathBuf;

use anyhow::Result;

use crate::{
    catalog,
    cli::Cli,
    git,
    model::{Catalog, Repo},
    paths::{absolute, repo_path},
};

/// Return the canonical destination derived from the catalog root and identity.
pub fn managed_path(catalog: &Catalog, repo: &Repo) -> Result<PathBuf> {
    absolute(&repo_path(catalog, repo)?)
}

/// Return an explicitly remembered path, even if the directory is no longer valid.
pub fn remembered_path(cli: &Cli, repo: &Repo) -> Result<Option<PathBuf>> {
    catalog::remembered_local_path(cli, &repo.source)?
        .map(|path| absolute(&path))
        .transpose()
}

/// Return the present local clone, preferring a remembered existing location.
///
/// If the observation is stale, Shu falls back to the managed destination so
/// a restored or migrated clone continues to work without cache cleanup.
pub fn present_path(cli: &Cli, catalog: &Catalog, repo: &Repo) -> Result<Option<PathBuf>> {
    if let Some(path) = remembered_path(cli, repo)?
        && git::is_repo(&path)
    {
        return Ok(Some(path));
    }
    let managed = managed_path(catalog, repo)?;
    Ok(git::is_repo(&managed).then_some(managed))
}
