//! Self-updating from checksummed GitHub Release binaries.

use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use std::process::Command;

use crate::{cli::UpgradeArgs, hash::sha256_hex, http, ui};
use anyhow::{Context, Result, anyhow, bail};
use ureq::ResponseExt;

const RELEASE_REPOSITORY: &str = "wiedymi/shu";
const PROGRESS_INTERVAL_BYTES: u64 = 256 * 1024;

/// Download a verified Shu release and replace the currently running binary.
///
/// GitHub Releases publish one raw executable per supported platform alongside
/// `SHA256SUMS`. Unix platforms replace the executable immediately. Windows
/// starts a short-lived PowerShell helper, which waits for Shu to exit before
/// replacing the locked executable.
pub fn upgrade(args: &UpgradeArgs) -> Result<()> {
    let target = release_target()?;
    let current = env::current_exe().context("could not determine Shu executable path")?;
    let asset = format!("shu-{target}{}", executable_extension());
    ui::heading("Upgrade");
    ui::detail("platform", target);
    let client = http::agent();
    let (version, manifest) = match args
        .version
        .as_deref()
        .filter(|version| *version != "latest")
    {
        Some(requested) => {
            let version = normalize_tag(requested);
            ui::working("Downloading release manifest");
            let manifest = get_text(&client, &format!("{}/SHA256SUMS", release_base(&version)))?;
            (version, manifest)
        }
        None => {
            ui::working("Finding latest release");
            latest_release_manifest(&client)?
        }
    };
    ui::detail(
        "version",
        format!("{} → {version}", env!("CARGO_PKG_VERSION")),
    );
    ui::working("Reading release manifest");
    let expected = checksum_for(&manifest, &asset)?;
    ui::action(format!("Downloading {asset}"));
    let binary = get_bytes(&client, &format!("{}/{asset}", release_base(&version)))?;
    ui::working("Verifying checksum");
    verify_checksum(&binary, &expected, &asset)?;

    ui::action("Install verified release");
    let staged = staged_path(&current)?;
    fs::write(&staged, binary)
        .with_context(|| format!("could not write downloaded binary {}", staged.display()))?;
    make_executable(&staged)?;

    #[cfg(windows)]
    replace_on_windows(&current, &staged)?;
    #[cfg(not(windows))]
    replace_now(&current, &staged)?;

    ui::success(format!("Updated Shu to {version}"));
    #[cfg(windows)]
    ui::detail(
        "location",
        format!("replaces {} after this process exits", current.display()),
    );
    #[cfg(not(windows))]
    ui::detail("location", current.display());
    Ok(())
}

/// Build a GitHub Release download base for the latest or one named release.
fn release_base(version: &str) -> String {
    format!("https://github.com/{RELEASE_REPOSITORY}/releases/download/{version}")
}

/// Download the latest manifest and derive its tag from GitHub's asset redirect.
fn latest_release_manifest(client: &ureq::Agent) -> Result<(String, String)> {
    let url =
        format!("https://github.com/{RELEASE_REPOSITORY}/releases/latest/download/SHA256SUMS");
    let mut response = http::get_with_redirect_history(client, &url, "latest release manifest")?;
    let version = response
        .get_redirect_history()
        .into_iter()
        .flatten()
        .find_map(|uri| release_tag_from_uri(uri.path()))
        .ok_or_else(|| {
            anyhow!("could not determine the latest release version from GitHub's redirect")
        })?;
    let manifest = response
        .body_mut()
        .read_to_string()
        .context("could not read latest release manifest")?;
    Ok((version, manifest))
}

/// Extract a version tag from GitHub's stable release download path.
fn release_tag_from_uri(path: &str) -> Option<String> {
    let segments = path.split('/').collect::<Vec<_>>();
    segments.windows(4).find_map(|window| match window {
        ["releases", "download", tag, "SHA256SUMS"] if *tag != "latest" => Some(normalize_tag(tag)),
        _ => None,
    })
}

/// Keep release URLs and human output consistently tag-shaped.
fn normalize_tag(version: &str) -> String {
    format!("v{}", version.trim_start_matches('v'))
}

/// Return the release target matching the currently executing binary.
fn release_target() -> Result<&'static str> {
    match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-musl"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-musl"),
        (os, architecture) => bail!("Shu upgrades are not published for {os}/{architecture}"),
    }
}

/// Return the platform's executable filename extension.
fn executable_extension() -> &'static str {
    if cfg!(windows) { ".exe" } else { "" }
}

/// Download text from an HTTPS release asset with an informative error.
fn get_text(client: &ureq::Agent, url: &str) -> Result<String> {
    http::get_text(client, url, "release manifest").with_context(|| {
        format!(
            "release asset was unavailable: {url}. Check your internet connection and release access"
        )
    })
}

/// Download a binary release asset while reporting its progress to the terminal.
fn get_bytes(client: &ureq::Agent, url: &str) -> Result<Vec<u8>> {
    let mut response = http::get(client, url, "release asset").with_context(|| {
            format!(
                "release asset was unavailable: {url}. Check your internet connection and release access"
            )
        })?;
    let content_length = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let capacity = content_length
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or_default();
    let mut bytes = Vec::with_capacity(capacity);
    let mut progress = DownloadProgress::new(content_length);
    let mut reader = response.body_mut().as_reader();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .context("could not read downloaded binary")?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        progress.advance(read as u64)?;
    }
    progress.finish()?;
    Ok(bytes)
}

/// Render compact, rate-limited binary download progress on standard error.
struct DownloadProgress {
    /// Advertised response size, when the server provides one.
    total: Option<u64>,
    /// Number of bytes received so far.
    downloaded: u64,
    /// Number of bytes shown in the previous update.
    last_rendered: u64,
}

impl DownloadProgress {
    /// Create a reporter for one response body.
    fn new(total: Option<u64>) -> Self {
        Self {
            total,
            downloaded: 0,
            last_rendered: 0,
        }
    }

    /// Record downloaded bytes and refresh an interactive progress line when useful.
    fn advance(&mut self, count: u64) -> Result<()> {
        self.downloaded += count;
        if self.downloaded - self.last_rendered >= PROGRESS_INTERVAL_BYTES {
            self.render()?;
        }
        Ok(())
    }

    /// Complete the progress display with a final, stable line.
    fn finish(&mut self) -> Result<()> {
        self.render()?;
        eprintln!();
        Ok(())
    }

    /// Rewrite one interactive line with the transferred byte count and percentage.
    fn render(&mut self) -> Result<()> {
        ui::render_download_progress(self.downloaded, self.total)?;
        self.last_rendered = self.downloaded;
        Ok(())
    }
}

/// Format a byte count for a short human-facing progress message.
/// Extract one hexadecimal checksum from a standard `SHA256SUMS` manifest.
fn checksum_for(manifest: &str, asset: &str) -> Result<String> {
    manifest
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let checksum = fields.next()?;
            let filename = fields.next()?.trim_start_matches('*');
            (filename == asset).then(|| checksum.to_owned())
        })
        .ok_or_else(|| anyhow!("release manifest does not contain {asset}"))
}

/// Verify that downloaded bytes match their published SHA-256 digest.
fn verify_checksum(bytes: &[u8], expected: &str, asset: &str) -> Result<()> {
    let actual = sha256_hex(bytes);
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        bail!("checksum verification failed for {asset}")
    }
}

/// Pick a sibling temporary path, ensuring replacement stays on the same filesystem.
fn staged_path(current: &Path) -> Result<PathBuf> {
    let parent = current
        .parent()
        .ok_or_else(|| anyhow!("Shu executable path has no parent directory"))?;
    let name = current
        .file_name()
        .ok_or_else(|| anyhow!("Shu executable path has no file name"))?
        .to_string_lossy();
    Ok(parent.join(format!(".{name}.upgrade-{}", std::process::id())))
}

/// Mark a downloaded Unix executable as runnable; Windows does not use mode bits.
fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(windows)]
    let _ = path;
    Ok(())
}

/// Atomically replace a Unix executable after its verified replacement is staged.
#[cfg(not(windows))]
fn replace_now(current: &Path, staged: &Path) -> Result<()> {
    fs::rename(staged, current).with_context(|| {
        format!(
            "could not replace {}; check that its installation directory is writable",
            current.display()
        )
    })
}

/// Start a PowerShell helper that replaces the executable after Shu exits.
#[cfg(windows)]
fn replace_on_windows(current: &Path, staged: &Path) -> Result<()> {
    let quote = |path: &Path| format!("'{}'", path.display().to_string().replace('\'', "''"));
    let script = format!(
        "$source = {}; $destination = {}; for ($attempt = 0; $attempt -lt 40; $attempt++) {{ try {{ Move-Item -LiteralPath $source -Destination $destination -Force; exit 0 }} catch {{ Start-Sleep -Milliseconds 250 }} }}; exit 1",
        quote(staged),
        quote(current),
    );
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .spawn()
        .context("could not start the Windows replacement helper")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_standard_checksum_lines() {
        let manifest = "abc  shu-aarch64-apple-darwin\ndef *shu-x86_64-pc-windows-msvc.exe\n";
        assert_eq!(
            checksum_for(manifest, "shu-x86_64-pc-windows-msvc.exe").unwrap(),
            "def"
        );
    }

    #[test]
    fn normalizes_release_tags_for_download_urls() {
        assert_eq!(normalize_tag("0.1.0"), "v0.1.0");
        assert_eq!(normalize_tag("v0.1.0"), "v0.1.0");
        assert!(release_base("v0.1.0").ends_with("releases/download/v0.1.0"));
    }

    #[test]
    fn reads_latest_tag_from_github_release_redirect() {
        assert_eq!(
            release_tag_from_uri("/wiedymi/shu/releases/download/v0.1.17/SHA256SUMS"),
            Some("v0.1.17".to_owned())
        );
        assert_eq!(
            release_tag_from_uri("/wiedymi/shu/releases/latest/download/SHA256SUMS"),
            None
        );
    }

    #[test]
    fn formats_download_sizes_for_humans() {
        assert_eq!(ui::human_size(512), "512 B");
        assert_eq!(ui::human_size(1536), "1.5 KiB");
        assert_eq!(ui::human_size(5 * 1024 * 1024), "5.0 MiB");
    }
}
