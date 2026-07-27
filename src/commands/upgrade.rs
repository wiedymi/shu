//! Self-updating from checksummed GitHub Release binaries.

use std::{
    env, fs,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
};

#[cfg(windows)]
use std::process::Command;

use crate::{cli::UpgradeArgs, hash::sha256_hex, http};
use anyhow::{Context, Result, anyhow, bail};

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
    let base = release_base(args.version.as_deref());
    eprintln!("\nShu upgrade\n");
    eprintln!("  Platform: {target}");
    eprintln!("  Release: {}", args.version.as_deref().unwrap_or("latest"));
    let client = http::agent();
    eprintln!("  Downloading SHA256SUMS");
    let manifest = get_text(&client, &format!("{base}/SHA256SUMS"))?;
    eprintln!("  Reading release manifest");
    let expected = checksum_for(&manifest, &asset)?;
    eprintln!("  Downloading {asset}");
    let binary = get_bytes(&client, &format!("{base}/{asset}"), &asset)?;
    eprintln!("  Verifying checksum");
    verify_checksum(&binary, &expected, &asset)?;

    eprintln!("  Installing");
    let staged = staged_path(&current)?;
    fs::write(&staged, binary)
        .with_context(|| format!("could not write downloaded binary {}", staged.display()))?;
    make_executable(&staged)?;

    #[cfg(windows)]
    replace_on_windows(&current, &staged)?;
    #[cfg(not(windows))]
    replace_now(&current, &staged)?;

    println!("✓ Shu upgrade downloaded successfully.");
    #[cfg(windows)]
    println!(
        "It will replace {} after this process exits.",
        current.display()
    );
    #[cfg(not(windows))]
    println!("Updated {}.", current.display());
    Ok(())
}

/// Build a GitHub Release download base for the latest or one named release.
fn release_base(version: Option<&str>) -> String {
    match version {
        None | Some("latest") => {
            format!("https://github.com/{RELEASE_REPOSITORY}/releases/latest/download")
        }
        Some(version) => {
            let tag = version.strip_prefix('v').unwrap_or(version);
            format!("https://github.com/{RELEASE_REPOSITORY}/releases/download/v{tag}")
        }
    }
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
fn get_bytes(client: &ureq::Agent, url: &str, asset: &str) -> Result<Vec<u8>> {
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
    let mut progress = DownloadProgress::new(asset, content_length);
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
struct DownloadProgress<'a> {
    /// Human-readable release asset name.
    asset: &'a str,
    /// Advertised response size, when the server provides one.
    total: Option<u64>,
    /// Number of bytes received so far.
    downloaded: u64,
    /// Number of bytes shown in the previous update.
    last_rendered: u64,
    /// Whether standard error supports an in-place progress line.
    interactive: bool,
}

impl<'a> DownloadProgress<'a> {
    /// Create a reporter for one response body.
    fn new(asset: &'a str, total: Option<u64>) -> Self {
        Self {
            asset,
            total,
            downloaded: 0,
            last_rendered: 0,
            interactive: io::stderr().is_terminal(),
        }
    }

    /// Record downloaded bytes and refresh an interactive progress line when useful.
    fn advance(&mut self, count: u64) -> Result<()> {
        self.downloaded += count;
        if self.interactive && self.downloaded - self.last_rendered >= PROGRESS_INTERVAL_BYTES {
            self.render()?;
        }
        Ok(())
    }

    /// Complete the progress display with a final, stable line.
    fn finish(&mut self) -> Result<()> {
        if self.interactive {
            self.render()?;
            eprintln!();
        } else {
            eprintln!(
                "  Downloaded {} ({})",
                self.asset,
                human_size(self.downloaded)
            );
        }
        Ok(())
    }

    /// Rewrite one interactive line with the transferred byte count and percentage.
    fn render(&mut self) -> Result<()> {
        let detail = match self.total {
            Some(total) if total > 0 => format!(
                "{} / {} ({}%)",
                human_size(self.downloaded),
                human_size(total),
                self.downloaded.saturating_mul(100) / total
            ),
            _ => human_size(self.downloaded),
        };
        eprint!("\r  Downloading {}: {detail}", self.asset);
        io::stderr().flush()?;
        self.last_rendered = self.downloaded;
        Ok(())
    }
}

/// Format a byte count for a short human-facing progress message.
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

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
    fn keeps_latest_and_normalizes_version_tags() {
        assert!(release_base(None).ends_with("releases/latest/download"));
        assert!(release_base(Some("0.1.0")).ends_with("releases/download/v0.1.0"));
        assert!(release_base(Some("v0.1.0")).ends_with("releases/download/v0.1.0"));
    }

    #[test]
    fn formats_download_sizes_for_humans() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MiB");
    }
}
