//! Small, shared HTTPS helpers for Shu's catalog and release downloads.

use std::time::Duration;

use anyhow::{Context, Result};
use ureq::{
    Agent,
    tls::{RootCerts, TlsConfig},
};

const USER_AGENT: &str = concat!("shu/", env!("CARGO_PKG_VERSION"));

/// Create Shu's HTTP client with the operating system's certificate verifier.
///
/// Catalog sources can point at arbitrary HTTPS hosts. Using the platform
/// verifier respects the user's current trust store rather than embedding a
/// static root bundle in every Shu binary.
pub fn agent() -> Agent {
    Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent()
}

/// Send an HTTPS GET request with Shu's user agent and contextual errors.
pub fn get(agent: &Agent, url: &str, purpose: &str) -> Result<ureq::http::Response<ureq::Body>> {
    agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("could not download {purpose}: {url}"))
}

/// Send an HTTPS GET while retaining the redirect chain for callers that need
/// the canonical location selected by a service.
pub fn get_with_redirect_history(
    agent: &Agent,
    url: &str,
    purpose: &str,
) -> Result<ureq::http::Response<ureq::Body>> {
    agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .config()
        .save_redirect_history(true)
        .build()
        .call()
        .with_context(|| format!("could not download {purpose}: {url}"))
}

/// Download a UTF-8 response body into memory.
pub fn get_text(agent: &Agent, url: &str, purpose: &str) -> Result<String> {
    let mut response = get(agent, url, purpose)?;
    response
        .body_mut()
        .read_to_string()
        .with_context(|| format!("could not read {purpose} response"))
}
