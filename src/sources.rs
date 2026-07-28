//! Catalog-source resolution for local files, direct URLs, and Gists.

use std::{fs, path::Path};

use crate::{http, identity::normalize_identity};
use anyhow::{Context, Result, anyhow};

/// Resolve a read-only catalog source into TOML text.
///
/// Git catalog sources are handled by `restore`, which keeps a normal Git
/// checkout under the catalog root rather than an opaque cache.
pub fn resolve(source: &str, file: Option<&Path>, _git_ref: Option<&str>) -> Result<String> {
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
    anyhow::bail!("unsupported catalog source: {source}")
}

/// Return the transport URL used for a Git-backed catalog source.
pub fn repository_remote(source: &str) -> Result<Option<String>> {
    if !is_repository_source(source) {
        return Ok(None);
    }
    let identity = normalize_identity(source)?;
    Ok(Some(
        if source.contains("://") || source.starts_with("git@") {
            source.to_owned()
        } else {
            format!("https://{identity}.git")
        },
    ))
}

fn is_repository_source(source: &str) -> bool {
    !Path::new(source).is_file()
        && (!source.starts_with("http://") && !source.starts_with("https://")
            || (!source.ends_with(".toml") && !source.contains("gist.github.com/")))
}

/// Download a direct TOML URL with an explicit Shu user agent.
fn http_get(url: &str) -> Result<String> {
    http::get_text(&http::agent(), url, "catalog source")
}

/// Read one named catalog file from a public GitHub Gist.
fn fetch_gist(source: &str, file: Option<&Path>) -> Result<String> {
    let id = source
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .ok_or_else(|| anyhow!("invalid Gist URL"))?;
    let api = format!("https://api.github.com/gists/{id}");
    let response = http::get_text(&http::agent(), &api, "Gist catalog")?;
    let value: serde_json::Value =
        serde_json::from_str(&response).context("could not parse the Gist catalog response")?;
    let wanted = file
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("shu.toml");
    value["files"][wanted]["content"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("Gist does not contain {wanted}"))
}
