//! Self-updating from checksummed GitHub Release binaries.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};

use crate::cli::UpgradeArgs;

const RELEASE_REPOSITORY: &str = "wiedymi/shu";

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
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("shu/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let manifest = get_text(&client, &format!("{base}/SHA256SUMS"))?;
    let expected = checksum_for(&manifest, &asset)?;
    let binary = get_bytes(&client, &format!("{base}/{asset}"))?;
    verify_checksum(&binary, &expected, &asset)?;

    let staged = staged_path(&current)?;
    fs::write(&staged, binary)
        .with_context(|| format!("could not write downloaded binary {}", staged.display()))?;
    make_executable(&staged)?;

    #[cfg(windows)]
    replace_on_windows(&current, &staged)?;
    #[cfg(not(windows))]
    replace_now(&current, &staged)?;

    println!("Shu upgrade downloaded successfully.");
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
fn get_text(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
    client
        .get(url)
        .send()
        .with_context(|| format!("could not download {url}"))?
        .error_for_status()
        .with_context(|| format!("release asset was unavailable: {url}"))?
        .text()
        .context("release manifest was not valid text")
}

/// Download binary data from an HTTPS release asset with an informative error.
fn get_bytes(client: &reqwest::blocking::Client, url: &str) -> Result<Vec<u8>> {
    client
        .get(url)
        .send()
        .with_context(|| format!("could not download {url}"))?
        .error_for_status()
        .with_context(|| format!("release asset was unavailable: {url}"))?
        .bytes()
        .map(|body| body.to_vec())
        .context("could not read downloaded binary")
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
    let actual = format!("{:x}", Sha256::digest(bytes));
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
}
