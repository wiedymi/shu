//! Catalog-source resolution for local files, direct URLs, Gists, and Git repositories.

use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};

use crate::{catalog::state_dir, identity::normalize_identity};

/// Resolve a supported catalog source into its TOML text.
///
/// A source may be a local path, direct TOML URL, Gist URL, or Git repository.
/// Git-backed catalogs use a Shu-owned cache so user repositories are never
/// modified while refreshing a source.
pub fn resolve(source: &str, file: Option<&Path>, git_ref: Option<&str>) -> Result<String> {
    if Path::new(source).is_file() {
        return fs::read_to_string(source)
            .with_context(|| format!("could not read catalog source {source}"));
    }
    if source.starts_with("http://") || source.starts_with("https://") {
        if source.contains("gist.github.com/") {
            return fetch_gist(source, file);
        }
        if source.ends_with(".toml") {
            return http_get(source);
        }
    }
    fetch_repository(source, file, git_ref)
}

/// Fetch a Git-backed catalog into Shu-owned cache state and read its TOML file.
fn fetch_repository(source: &str, file: Option<&Path>, git_ref: Option<&str>) -> Result<String> {
    let identity = normalize_identity(source)?;
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    let key = format!("{:x}", hasher.finalize());
    let cache = state_dir()?.join("catalogs").join(&key[..16]);
    if !cache.exists() {
        fs::create_dir_all(cache.parent().unwrap())?;
        let remote = if source.contains("://") || source.starts_with("git@") {
            source.to_owned()
        } else {
            format!("https://{}.git", identity)
        };
        let mut command = Command::new("git");
        command.args(["clone", "--depth", "1"]);
        if let Some(reference) = git_ref {
            command.args(["--branch", reference]);
        }
        if !command
            .arg(remote)
            .arg(&cache)
            .status()
            .context("could not clone catalog repository")?
            .success()
        {
            bail!("could not clone catalog repository {source}");
        }
    } else {
        if !Command::new("git")
            .arg("-C")
            .arg(&cache)
            .args(["fetch", "--depth", "1", "origin"])
            .status()?
            .success()
        {
            bail!("could not refresh catalog repository {source}");
        }
        // This is Shu-owned cache state, never a user repository.
        if !Command::new("git")
            .arg("-C")
            .arg(&cache)
            .args(["reset", "--hard", "FETCH_HEAD"])
            .status()?
            .success()
        {
            bail!("could not update cached catalog repository {source}");
        }
    }
    let filename = file.unwrap_or_else(|| Path::new("shu.toml"));
    fs::read_to_string(cache.join(filename)).with_context(|| {
        format!(
            "catalog file {} was not found in {source}",
            filename.display()
        )
    })
}

/// Download a direct TOML URL with an explicit Shu user agent.
fn http_get(url: &str) -> Result<String> {
    reqwest::blocking::Client::builder()
        .user_agent("shu/0.1")
        .build()?
        .get(url)
        .send()?
        .error_for_status()?
        .text()
        .context("could not read catalog response")
}

/// Read one named catalog file from a public GitHub Gist.
fn fetch_gist(source: &str, file: Option<&Path>) -> Result<String> {
    let id = source
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .ok_or_else(|| anyhow!("invalid Gist URL"))?;
    let api = format!("https://api.github.com/gists/{id}");
    let value: serde_json::Value = reqwest::blocking::Client::builder()
        .user_agent("shu/0.1")
        .build()?
        .get(api)
        .send()?
        .error_for_status()?
        .json()?;
    let wanted = file
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("shu.toml");
    value["files"][wanted]["content"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("Gist does not contain {wanted}"))
}
