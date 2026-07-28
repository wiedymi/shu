//! `shu.toml` types and machine-readable output structures.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
/// A user-declared lifecycle state; it is never inferred from local activity.
pub enum Lifecycle {
    Active,
    Parked,
    Reference,
    Archived,
}

impl std::fmt::Display for Lifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Active => "active",
                Self::Parked => "parked",
                Self::Reference => "reference",
                Self::Archived => "archived",
            }
        )
    }
}

#[derive(Debug, Deserialize, Serialize)]
/// The complete user-facing Shu configuration stored in `shu.toml`.
pub struct Catalog {
    /// Catalog format version.
    pub version: u32,
    /// Base directory containing canonical repository paths.
    #[serde(default = "default_root")]
    pub root: String,
    /// Repositories preserved by this catalog.
    #[serde(default)]
    pub repos: Vec<Repo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync: Option<Sync>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
/// One catalogued repository, its human-maintained metadata, and local clones.
pub struct Repo {
    /// Normalized `host/namespace/repository` identity.
    pub source: String,
    /// Optional SSH transport preserved for cloning this repository.
    ///
    /// The canonical identity remains in [`Self::source`]. This field exists
    /// only when the user explicitly added an SSH remote, so later restores do
    /// not unexpectedly switch an SSH-only repository to HTTPS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    /// User-declared lifecycle state.
    #[serde(default = "default_state")]
    pub state: Lifecycle,
    /// Free-form labels for filtering and grouping.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Optional context explaining why the repository is retained.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Full clone roots known for this repository on the current machine.
    ///
    /// Paths are intentionally stored in the single `shu.toml` catalog so the
    /// user can inspect and edit all Shu state in one readable file. Missing
    /// paths are harmless on another machine; Shu simply ignores them until a
    /// valid local clone exists.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    /// Preferred local clone path used when a command needs one destination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
}

impl Repo {
    /// Add a clone path once, preserving the order in which paths were added.
    pub fn add_path(&mut self, path: PathBuf) {
        let value = path.display().to_string();
        if !self.paths.iter().any(|known| known == &value) {
            self.paths.push(value);
        }
    }

    /// Replace a moved clone path and make its destination the preferred clone.
    pub fn replace_path(&mut self, source: &Path, destination: PathBuf) {
        let source = source.display().to_string();
        let destination = destination.display().to_string();
        self.paths.retain(|known| known != &source);
        if !self.paths.iter().any(|known| known == &destination) {
            self.paths.push(destination.clone());
        }
        self.primary = Some(destination);
    }

    /// Return the explicit preferred clone path, if the catalog has one.
    pub fn primary_path(&self) -> Option<PathBuf> {
        self.primary.as_ref().map(PathBuf::from)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
/// Git transport settings stored with the portable catalog.
pub struct Sync {
    /// The Git repository that owns this catalog.
    pub remote: String,
    #[serde(default = "default_sync_file")]
    pub file: String,
    #[serde(default = "default_sync_ref")]
    pub r#ref: String,
}

fn default_sync_file() -> String {
    "shu.toml".into()
}

fn default_sync_ref() -> String {
    "main".into()
}

#[derive(Debug, Serialize)]
/// Versioned envelope for machine-readable repository listings.
pub struct ListOutput {
    /// JSON schema version for this response shape.
    pub schema_version: u32,
    /// Repository records in this response.
    pub repositories: Vec<RepoOutput>,
}

#[derive(Debug, Serialize)]
/// Machine-readable view of one catalog entry and its local state.
pub struct RepoOutput {
    /// Normalized repository identity.
    pub identity: String,
    /// Last component of the repository identity.
    pub name: String,
    /// Present observed path, or the canonical destination when missing.
    pub path: String,
    /// Lifecycle state stored in the catalog.
    pub declared_state: Lifecycle,
    /// State detected on the current machine.
    pub observed_state: String,
    /// Free-form catalog tags.
    pub tags: Vec<String>,
    /// Optional human-maintained note.
    pub note: Option<String>,
}

/// Default canonical root used for a newly initialized catalog.
pub fn default_root() -> String {
    "~/shu".into()
}
/// Default a newly added entry to active when its catalog field is omitted.
fn default_state() -> Lifecycle {
    Lifecycle::Active
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_shu_directory_as_the_default_root() {
        assert_eq!(default_root(), "~/shu");
    }
}
