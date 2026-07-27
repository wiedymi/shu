//! Repository-root expansion and canonical local-path construction.

use std::path::{Path, PathBuf};

use crate::{
    identity::normalize_identity,
    model::{Catalog, Repo},
};
use anyhow::{Result, anyhow};

pub fn root_path(catalog: &Catalog) -> Result<PathBuf> {
    expand_home(&catalog.root)
}
pub fn repo_path(catalog: &Catalog, repo: &Repo) -> Result<PathBuf> {
    Ok(root_path(catalog)?.join(normalize_identity(&repo.source)?))
}
pub fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

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
