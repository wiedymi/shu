//! Resolution of a repository's managed, remembered, and Git worktree paths.
//!
//! `shu.toml` owns repository identities and the clone paths Shu remembers
//! after `shu add .`. This module asks Git for linked worktrees when they are
//! needed, without ever storing worktree state itself.

use std::path::PathBuf;

use anyhow::Result;

use crate::{
    git,
    model::{Catalog, Repo},
    paths::{absolute, repo_path},
};

/// Return the canonical destination derived from the catalog root and identity.
pub fn managed_path(catalog: &Catalog, repo: &Repo) -> Result<PathBuf> {
    absolute(&repo_path(catalog, repo)?)
}

/// Return every remembered clone path, including paths that have gone stale.
pub fn remembered_paths(repo: &Repo) -> Result<Vec<PathBuf>> {
    repo.paths
        .iter()
        .map(PathBuf::from)
        .map(|path| absolute(&path))
        .collect()
}

/// Return the explicitly selected clone path, including a stale path.
pub fn primary_path(repo: &Repo) -> Result<Option<PathBuf>> {
    repo.primary_path().map(|path| absolute(&path)).transpose()
}

/// Return every valid full clone known for a repository on this machine.
///
/// Catalog paths preserve their registration order. The derived managed
/// location is included when it is a valid repository even if it is absent
/// from the catalog's `paths` list.
pub fn present_paths(catalog: &Catalog, repo: &Repo) -> Result<Vec<PathBuf>> {
    let mut paths = remembered_paths(repo)?
        .into_iter()
        .filter(|path| git::is_repo(path))
        .collect::<Vec<_>>();
    let managed = managed_path(catalog, repo)?;
    if git::is_repo(&managed) && !paths.iter().any(|path| path == &managed) {
        paths.push(managed);
    }
    Ok(paths)
}

/// Return the preferred present clone path for commands that require one path.
///
/// An explicitly selected, valid primary clone wins. Otherwise Shu uses the
/// first valid remembered path, then the canonical managed location.
pub fn present_path(catalog: &Catalog, repo: &Repo) -> Result<Option<PathBuf>> {
    if let Some(primary) = primary_path(repo)?
        && git::is_repo(&primary)
    {
        return Ok(Some(primary));
    }
    Ok(present_paths(catalog, repo)?.into_iter().next())
}

/// Return all present clone and linked-worktree paths for picker discovery.
///
/// Git worktrees remain Git-owned state: Shu reads them dynamically and does
/// not store them in `shu.toml`.
pub fn pickable_paths(catalog: &Catalog, repo: &Repo) -> Result<Vec<PathBuf>> {
    let clones = present_paths(catalog, repo)?;
    let primary = present_path(catalog, repo)?;
    let mut paths = Vec::new();
    if let Some(primary) = primary {
        paths.push(primary);
    }
    for clone in clones {
        push_unique(&mut paths, clone.clone());
        for worktree in git::worktrees(&clone)? {
            push_unique(&mut paths, worktree);
        }
    }
    Ok(paths)
}

/// Remove duplicate filesystem paths while retaining the first useful order.
fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|known| known == &path) {
        paths.push(path);
    }
}
