//! Narrow, safe wrappers around the user's installed `git` executable.
//!
//! Using system Git preserves existing credential helpers and keeps Shu out of
//! authentication and repository-transport concerns.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::identity::normalize_identity;
use anyhow::{Context, Result, anyhow, bail};

/// Return whether a path is an accessible Git working tree.
pub fn is_repo(path: &Path) -> bool {
    output(path, ["rev-parse", "--is-inside-work-tree"]).is_ok_and(|value| value == "true")
}

/// Return whether Git identifies this working tree as a shallow clone.
pub fn is_shallow(path: &Path) -> bool {
    output(path, ["rev-parse", "--is-shallow-repository"]).is_ok_and(|value| value == "true")
}

/// Return whether a working tree is checked out as a submodule of another repository.
pub fn is_submodule(path: &Path) -> bool {
    output(path, ["rev-parse", "--show-superproject-working-tree"])
        .is_ok_and(|value| !value.is_empty())
}

/// Resolve a local path inside a Git working tree to its top-level directory.
pub fn worktree_root(path: &Path) -> Result<PathBuf> {
    if !is_repo(path) {
        bail!("not a Git working tree: {}", path.display());
    }
    let root = PathBuf::from(output(path, ["rev-parse", "--show-toplevel"])?);
    let resolved = fs::canonicalize(&root)
        .with_context(|| format!("could not resolve Git working tree {}", root.display()))?;
    Ok(presentation_path(resolved))
}

/// Remove Windows' internal verbatim-path prefix before displaying or storing a path.
///
/// `canonicalize` may return `\\?\C:\...`, which is valid for Windows APIs but
/// surprising in user-facing catalog observations and shell navigation output.
#[cfg(windows)]
fn presentation_path(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = value.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path
}

/// Preserve canonical paths unchanged on platforms without Windows verbatim paths.
#[cfg(not(windows))]
fn presentation_path(path: PathBuf) -> PathBuf {
    path
}

/// Return whether a working tree has no staged, unstaged, or untracked changes.
pub fn is_clean(path: &Path) -> Result<bool> {
    Ok(output(path, ["status", "--porcelain=v1", "--untracked-files=all"])?.is_empty())
}

/// Return whether Git reports another linked working tree for this repository.
pub fn has_linked_worktrees(path: &Path) -> Result<bool> {
    Ok(reported_worktrees(path)?.len() > 1)
}

/// Return every present working-tree path Git associates with a local repository.
///
/// The first entry is normally the main working tree and later entries are
/// linked worktrees. Paths are read from Git each time so Shu never stores
/// worktree state in `shu.toml`. Git may retain prunable entries after a
/// temporary worktree disappears; those absent paths cannot be picked and do
/// not prevent the remaining repositories from being used.
pub fn worktrees(path: &Path) -> Result<Vec<PathBuf>> {
    reported_worktrees(path)?
        .into_iter()
        .map(|path| match fs::canonicalize(&path) {
            Ok(path) => Ok(Some(presentation_path(path))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error)
                .with_context(|| format!("could not resolve Git worktree {}", path.display())),
        })
        .collect::<Result<Vec<_>>>()
        .map(|paths| paths.into_iter().flatten().collect())
}

/// Return every worktree path in Git's metadata, including prunable entries.
fn reported_worktrees(path: &Path) -> Result<Vec<PathBuf>> {
    Ok(output(path, ["worktree", "list", "--porcelain"])?
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect())
}

/// Run Git in a working directory and return trimmed standard output on success.
pub fn output<const N: usize>(dir: &Path, args: [&str; N]) -> Result<String> {
    let result = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .with_context(|| "could not run git; ensure Git is installed")?;
    if !result.status.success() {
        bail!(
            "git command failed in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}

/// Compatibility alias for [`output`], used by catalog code.
pub fn git_output<const N: usize>(dir: &Path, args: [&str; N]) -> Result<String> {
    output(dir, args)
}

/// Clone a catalog source into an empty canonical target path.
///
/// Canonical identities use HTTPS. Explicit SSH remotes are preserved exactly
/// as supplied so Git can use the user's configured SSH credentials.
pub fn clone(source: &str, target: &Path) -> Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("target has no parent"))?;
    fs::create_dir_all(parent)?;
    let remote = clone_url(source)?;
    let output = Command::new("git")
        .args(["clone", "--", &remote])
        .arg(target)
        .stdin(Stdio::null())
        .output()
        .context("could not run git clone")?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!(
            "could not clone {source}. Check your internet connection and Git access to this repository. Git said: {detail}"
        );
    }
    Ok(())
}

/// Return the safe Git remote used to clone a catalog entry.
fn clone_url(source: &str) -> Result<String> {
    let source = source.trim();
    if source.starts_with("git@") || source.starts_with("ssh://") {
        return Ok(source.to_owned());
    }
    Ok(format!("https://{}.git", normalize_identity(source)?))
}

/// Return whether Git has an author identity available for commits.
pub fn has_author_identity() -> Result<bool> {
    if std::env::var_os("GIT_AUTHOR_NAME").is_some_and(|value| !value.is_empty())
        && std::env::var_os("GIT_AUTHOR_EMAIL").is_some_and(|value| !value.is_empty())
    {
        return Ok(true);
    }
    Ok(git_config_value("user.name")?.is_some() && git_config_value("user.email")?.is_some())
}

/// Return one effective Git configuration value without requiring a repository.
fn git_config_value(key: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["config", "--get", key])
        .output()
        .with_context(|| "could not run git; ensure Git is installed")?;
    if output.status.success() {
        return Ok(Some(String::from_utf8(output.stdout)?.trim().to_owned()));
    }
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    bail!(
        "could not read Git configuration: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

/// Return whether a remote already has branch heads.
pub fn remote_has_heads(remote: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["ls-remote", "--heads", remote])
        .stdin(Stdio::null())
        .output()
        .with_context(|| "could not run git ls-remote")?;
    if !output.status.success() {
        bail!(
            "could not inspect remote {remote}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(!output.stdout.is_empty())
}

/// Clone a catalog remote with the user's existing Git credentials.
pub fn clone_remote(remote: &str, target: &Path, git_ref: Option<&str>) -> Result<()> {
    let mut command = Command::new("git");
    command.arg("clone");
    if let Some(git_ref) = git_ref {
        command.args(["--branch", git_ref]);
    }
    let output = command
        .arg(remote)
        .arg(target)
        .stdin(Stdio::null())
        .output()
        .context("could not clone catalog source")?;
    if !output.status.success() {
        bail!(
            "could not clone catalog source: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Initialize an empty working tree with `main` as its initial branch.
pub fn initialize(target: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["init", "-b", "main"])
        .arg(target)
        .stdin(Stdio::null())
        .output()
        .context("could not run git init")?;
    if !output.status.success() {
        bail!(
            "could not initialize Git repository: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}
