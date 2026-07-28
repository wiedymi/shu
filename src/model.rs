//! `shu.toml` types and machine-readable output structures.

use std::collections::BTreeMap;

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
    /// Named, portable repository queries derived from repository tags.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub collections: BTreeMap<String, Collection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync: Option<Sync>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
/// A named repository group derived from tags instead of stored membership.
pub struct Collection {
    /// Tags every repository in this collection must have.
    #[serde(default)]
    pub tags: Vec<String>,
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
    /// Clone paths known on the current machine.
    ///
    /// Managed paths are stored relative to the local catalog root. External
    /// paths are absolute and deliberately excluded from the synced catalog.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    /// Preferred local clone path used when a command needs one destination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
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
