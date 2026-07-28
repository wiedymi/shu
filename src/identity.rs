//! Normalization for the Git remote formats Shu accepts.

use anyhow::{Result, anyhow, bail};

/// Normalize a Git remote or shorthand into `host/namespace/repository`.
///
/// HTTPS URLs, SSH URLs, SCP-style SSH remotes, and already-normalized values
/// are accepted. Transport syntax and a final `.git` suffix are removed.
pub fn normalize_identity(input: &str) -> Result<String> {
    let raw = trim_remote_suffix(input);
    let identity = parse_identity(raw, input)?;
    validate_identity(&identity, input)?;
    Ok(identity.render())
}

/// The host and repository path extracted from one supported remote format.
struct ParsedIdentity {
    /// Remote host, before lowercasing for canonical output.
    host: String,
    /// Namespace and repository components, without transport syntax.
    path: String,
}

impl ParsedIdentity {
    /// Render this parsed value in Shu's canonical identity format.
    fn render(self) -> String {
        format!("{}/{}", self.host.to_ascii_lowercase(), self.path)
    }
}

/// Remove whitespace, a trailing slash, and one conventional Git suffix.
fn trim_remote_suffix(input: &str) -> &str {
    input.trim().trim_end_matches('/').trim_end_matches(".git")
}

/// Parse one of Shu's supported remote forms into host and repository path.
fn parse_identity(raw: &str, original: &str) -> Result<ParsedIdentity> {
    if let Some(remote) = raw.strip_prefix("git@") {
        parse_scp_remote(remote, original)
    } else if raw.contains("://") {
        parse_url_remote(raw, original)
    } else {
        parse_shorthand(raw, original)
    }
}

/// Parse a Git SCP-style remote such as `git@github.com:owner/project`.
fn parse_scp_remote(remote: &str, original: &str) -> Result<ParsedIdentity> {
    let (host, path) = remote
        .split_once(':')
        .ok_or_else(|| anyhow!("invalid SSH Git URL: {original}"))?;
    Ok(ParsedIdentity {
        host: host.to_owned(),
        path: path.trim_matches('/').trim_end_matches(".git").to_owned(),
    })
}

/// Parse a standard URL such as `https://github.com/owner/project`.
fn parse_url_remote(raw: &str, original: &str) -> Result<ParsedIdentity> {
    let (_, remainder) = raw
        .split_once("://")
        .ok_or_else(|| anyhow!("invalid URL: {original}"))?;
    let (authority, path) = remainder
        .split_once('/')
        .ok_or_else(|| anyhow!("Git URL has no repository path: {original}"))?;
    let host_and_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
        .trim();
    let host = host_and_port
        .split_once(':')
        .map_or(host_and_port, |(host, _)| host);
    Ok(ParsedIdentity {
        host: host.to_owned(),
        path: path
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .trim_matches('/')
            .trim_end_matches(".git")
            .to_owned(),
    })
}

/// Parse the plain `host/namespace/repository` form used in catalogs.
fn parse_shorthand(raw: &str, original: &str) -> Result<ParsedIdentity> {
    let (host, path) = raw.trim_matches('/').split_once('/').ok_or_else(|| {
        anyhow!("repository identity must be host/namespace/repository: {original}")
    })?;
    if !path.contains('/') {
        bail!("repository identity must be host/namespace/repository: {original}");
    }
    Ok(ParsedIdentity {
        host: host.to_owned(),
        path: path.trim_matches('/').trim_end_matches(".git").to_owned(),
    })
}

/// Reject incomplete or path-unsafe parsed identities before they reach the filesystem.
fn validate_identity(identity: &ParsedIdentity, original: &str) -> Result<()> {
    if !valid_host(&identity.host) || !valid_repository_path(&identity.path) {
        bail!("invalid repository identity: {original}");
    }
    Ok(())
}

/// Return whether a host is non-empty, path-free, and made of safe labels.
fn valid_host(host: &str) -> bool {
    !host.is_empty() && !host.contains(['/', '\\']) && host.split('.').all(valid_component)
}

/// Return whether a repository path contains at least namespace and repository components.
fn valid_repository_path(path: &str) -> bool {
    let mut parts = path.split('/');
    parts.next().is_some_and(valid_component)
        && parts.next().is_some_and(valid_component)
        && parts.all(valid_component)
}

/// Return whether a single identity component is safe to use below the repository root.
fn valid_component(value: &str) -> bool {
    !value.is_empty() && value != "." && value != ".." && !value.contains('\\')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_common_remote_forms() {
        for value in [
            "https://github.com/acme/widgets.git",
            "git@github.com:acme/widgets.git",
            "ssh://git@github.com/acme/widgets.git",
            "ssh://git@github.com:2222/acme/widgets.git",
            "https://github.com/acme/widgets.git?ref=main#readme",
            "github.com/acme/widgets",
        ] {
            assert_eq!(
                normalize_identity(value).unwrap(),
                "github.com/acme/widgets"
            );
        }
    }

    #[test]
    fn rejects_short_identity() {
        assert!(normalize_identity("acme/widgets").is_err());
    }

    #[test]
    fn rejects_identity_path_traversal() {
        for value in [
            "../owner/repository",
            "..\\owner/repository",
            "github.com/../repository",
            "github.com/owner/../repository",
        ] {
            assert!(normalize_identity(value).is_err(), "should reject {value}");
        }
    }
}
