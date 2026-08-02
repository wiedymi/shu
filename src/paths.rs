//! Filesystem-path validation, expansion, and canonical local-path construction.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use crate::{
    identity::normalize_identity,
    model::{Catalog, Repo},
};
use anyhow::{Result, anyhow};

/// Expand the catalog root, including a leading home-directory marker.
pub fn root_path(catalog: &Catalog) -> Result<PathBuf> {
    expand_home(&catalog.root)
}
/// Derive a repository's canonical local path below the catalog root.
pub fn repo_path(catalog: &Catalog, repo: &Repo) -> Result<PathBuf> {
    Ok(root_path(catalog)?.join(normalize_identity(&repo.source)?))
}
/// Convert a path to an absolute path without resolving symlinks.
pub fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

/// Return an XDG configuration root only when its environment value is absolute.
pub fn valid_xdg_config_home(value: Option<OsString>) -> Option<PathBuf> {
    value.map(PathBuf::from).filter(|path| path.is_absolute())
}

/// Expand `~`, `~/`, and `~\\` prefixes using the current user's home directory.
fn expand_home(value: &str) -> Result<PathBuf> {
    if value == "~" || value.starts_with("~/") || value.starts_with("~\\") {
        let home = directories::BaseDirs::new()
            .ok_or_else(|| anyhow!("could not determine home directory"))?
            .home_dir()
            .to_path_buf();
        return Ok(home.join(value[2..].replace('/', std::path::MAIN_SEPARATOR_STR)));
    }
    Ok(PathBuf::from(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_empty_and_relative_xdg_config_homes() {
        for value in [
            None,
            Some(OsString::new()),
            Some(OsString::from("relative/config")),
        ] {
            assert_eq!(valid_xdg_config_home(value), None);
        }
    }

    #[test]
    fn accepts_an_absolute_xdg_config_home() {
        let path = std::env::current_dir().unwrap().join("xdg-config");

        assert_eq!(
            valid_xdg_config_home(Some(path.clone().into_os_string())),
            Some(path)
        );
    }
}
