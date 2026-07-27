use serde::{Deserialize, Serialize};

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
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
pub struct Catalog {
    pub version: u32,
    #[serde(default = "default_root")]
    pub root: String,
    #[serde(default)]
    pub repos: Vec<Repo>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Repo {
    pub source: String,
    #[serde(default = "default_state")]
    pub state: Lifecycle,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Origin {
    pub source: String,
    pub file: Option<String>,
    pub git_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListOutput {
    pub schema_version: u32,
    pub repositories: Vec<RepoOutput>,
}

#[derive(Debug, Serialize)]
pub struct RepoOutput {
    pub identity: String,
    pub name: String,
    pub path: String,
    pub declared_state: Lifecycle,
    pub observed_state: String,
    pub tags: Vec<String>,
    pub note: Option<String>,
}

pub fn default_root() -> String {
    "~/Code".into()
}
fn default_state() -> Lifecycle {
    Lifecycle::Active
}
