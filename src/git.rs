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

/// Resolve a local path inside a Git working tree to its top-level directory.
pub fn worktree_root(path: &Path) -> Result<PathBuf> {
    if !is_repo(path) {
        bail!("not a Git working tree: {}", path.display());
    }
    let root = PathBuf::from(output(path, ["rev-parse", "--show-toplevel"])?);
    fs::canonicalize(&root)
        .with_context(|| format!("could not resolve Git working tree {}", root.display()))
}

/// Return whether a working tree has no staged, unstaged, or untracked changes.
pub fn is_clean(path: &Path) -> Result<bool> {
    Ok(output(path, ["status", "--porcelain=v1", "--untracked-files=all"])?.is_empty())
}

/// Return whether Git reports another linked working tree for this repository.
pub fn has_linked_worktrees(path: &Path) -> Result<bool> {
    let count = output(path, ["worktree", "list", "--porcelain"])?
        .lines()
        .filter(|line| line.starts_with("worktree "))
        .count();
    Ok(count > 1)
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

/// Clone an identity into an empty canonical target path using HTTPS transport.
pub fn clone(identity: &str, target: &Path) -> Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("target has no parent"))?;
    fs::create_dir_all(parent)?;
    let remote = format!("https://{}.git", normalize_identity(identity)?);
    let output = Command::new("git")
        .args(["clone", "--", &remote])
        .arg(target)
        .stdin(Stdio::null())
        .output()
        .context("could not run git clone")?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!(
            "could not clone {identity}. Check your internet connection and Git access to this repository. Git said: {detail}"
        );
    }
    Ok(())
}
