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
    if host.is_empty()
        || path.split('/').filter(|part| !part.is_empty()).count() < 2
        || path.contains("..")
    {
        bail!("invalid repository identity: {input}");
    }
    Ok(format!("{}/{}", host.to_ascii_lowercase(), path))
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
}
