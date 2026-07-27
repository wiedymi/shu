//! Normalization for common HTTPS and SSH Git remote formats.

use anyhow::{Context, Result, anyhow, bail};

/// Normalize a common Git remote form into `host/namespace/repository`.
///
/// HTTPS, SSH URL, SCP-style SSH, and already-normalized forms are accepted.
/// Transport-specific syntax and a final `.git` suffix are removed.
pub fn normalize_identity(input: &str) -> Result<String> {
    let raw = input.trim().trim_end_matches('/').trim_end_matches(".git");
    let (host, path) = if let Some(rest) = raw.strip_prefix("git@") {
        let (host, path) = rest
            .split_once(':')
            .ok_or_else(|| anyhow!("invalid SSH Git URL: {input}"))?;
        (host.to_owned(), path.to_owned())
    } else if raw.contains("://") {
        let url = url::Url::parse(raw).with_context(|| format!("invalid URL: {input}"))?;
        (
            url.host_str()
                .ok_or_else(|| anyhow!("Git URL has no host: {input}"))?
                .to_owned(),
            url.path().trim_matches('/').to_owned(),
        )
    } else {
        let parts = raw.trim_matches('/').split('/').collect::<Vec<_>>();
        if parts.len() < 3 {
            bail!("repository identity must be host/namespace/repository: {input}");
        }
        (parts[0].to_owned(), parts[1..].join("/"))
    };
    let path = path.trim_matches('/').trim_end_matches(".git");
    let valid_host =
        !host.is_empty() && !host.contains(['/', '\\']) && host.split('.').all(valid_component);
    let path_parts = path.split('/').collect::<Vec<_>>();
    if !valid_host || path_parts.len() < 2 || !path_parts.iter().all(|part| valid_component(part)) {
        bail!("invalid repository identity: {input}");
    }
    Ok(format!("{}/{}", host.to_ascii_lowercase(), path))
}

/// Return whether one host or repository-path component is safe to place in a path.
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
