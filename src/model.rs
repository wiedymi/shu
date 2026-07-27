//! Portable TOML catalog types and machine-readable output structures.

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
/// The portable desired state stored in `shu.toml`.
pub struct Catalog {
    /// Catalog format version.
    pub version: u32,
    /// Base directory containing canonical repository paths.
    #[serde(default = "default_root")]
    pub root: String,
    /// Repositories preserved by this catalog.
    #[serde(default)]
    pub repos: Vec<Repo>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
/// One desired repository and its human-maintained metadata.
pub struct Repo {
    /// Normalized `host/namespace/repository` identity.
    pub source: String,
    /// User-declared lifecycle state.
    #[serde(default = "default_state")]
    pub state: Lifecycle,
    /// Free-form labels for filtering and grouping.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Optional context explaining why the repository is retained.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
/// The saved origin used by `shu update`.
pub struct Origin {
    /// Original source string passed to `shu restore`.
    pub source: String,
    /// Optional file path within the source.
    pub file: Option<String>,
    /// Optional Git ref used to resolve the source.
    pub git_ref: Option<String>,
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
    /// Canonical absolute local path.
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
