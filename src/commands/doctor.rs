//! Read-only environment checks for setting up and maintaining Shu.

use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::{
    catalog,
    cli::{Cli, DoctorArgs},
    git,
    model::{Catalog, Sync},
    paths::root_path,
    ui,
};

/// Check the local Shu setup and report actionable failures.
///
/// The default checks never access the network or modify repositories. Passing
/// `--check-source` also verifies that the remembered Git source is reachable.
pub fn doctor(cli: &Cli, args: &DoctorArgs) -> Result<()> {
    let mut checks = Vec::new();
    checks.push(check_git());
    checks.push(check_git_author()?);

    let path = catalog::catalog_path(cli)?;
    let loaded = catalog::load(cli);
    let catalog = match loaded {
        Ok((_, catalog)) => {
            checks.push(Check::pass("catalog", format!("valid: {}", path.display())));
            Some(catalog)
        }
        Err(error) => {
            checks.push(Check::fail("catalog", format!("{}", error)));
            None
        }
    };

    if let Some(catalog) = catalog.as_ref() {
        checks.push(check_root(catalog)?);
    } else {
        checks.push(Check::skip("repository root", "catalog is unavailable"));
    }

    checks.push(check_sync(catalog.as_ref(), args.check_source)?);
    checks.push(check_github(args.check_github));
    render(cli, &checks)?;
    if checks.iter().any(|check| check.status == CheckStatus::Fail) {
        bail!("Shu setup has failing checks")
    }
    Ok(())
}

/// Verify optional GitHub CLI readiness only when explicitly requested.
fn check_github(check_github: bool) -> Check {
    if !check_github {
        return Check::skip(
            "GitHub CLI",
            "not checked; run `shu doctor --check-github` before `shu new --github`",
        );
    }
    match Command::new("gh").args(["auth", "status"]).output() {
        Ok(output) if output.status.success() => Check::pass(
            "GitHub CLI",
            "installed and authenticated for `shu new --github`",
        ),
        Ok(output) => Check::fail(
            "GitHub CLI",
            format!(
                "not authenticated: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ),
        Err(_) => Check::fail(
            "GitHub CLI",
            "gh is not installed; install it or create remotes manually",
        ),
    }
}

/// One named diagnostic result emitted by [`doctor`].
#[derive(Serialize)]
struct Check {
    /// Human-readable name of the checked component.
    name: &'static str,
    /// Stable outcome string for humans and JSON consumers.
    status: CheckStatus,
    /// Concise explanation or remediation hint.
    detail: String,
}

/// Versioned machine-readable envelope for a doctor report.
#[derive(Serialize)]
struct DoctorOutput<'a> {
    /// JSON schema version for this response shape.
    schema_version: u32,
    /// Individual setup checks.
    checks: &'a [Check],
}

impl Check {
    /// Create a successful check.
    fn pass(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Pass,
            detail: detail.into(),
        }
    }

    /// Create a failing check.
    fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Fail,
            detail: detail.into(),
        }
    }

    /// Create a check intentionally not run in this invocation.
    fn skip(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Skip,
            detail: detail.into(),
        }
    }
}

/// Outcome of one diagnostic check.
#[derive(PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum CheckStatus {
    /// The component is ready.
    Pass,
    /// The component needs user attention.
    Fail,
    /// The component was not applicable or was intentionally not checked.
    Skip,
}

/// Confirm that the installed Git executable can run.
fn check_git() -> Check {
    match Command::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => Check::pass(
            "git",
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        ),
        Ok(output) => Check::fail(
            "git",
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ),
        Err(error) => Check::fail("git", format!("could not run Git: {error}")),
    }
}

/// Confirm that Git can create the commits required by catalog sync.
fn check_git_author() -> Result<Check> {
    if git::has_author_identity()? {
        Ok(Check::pass("Git author", "configured for catalog commits"))
    } else {
        Ok(Check::fail(
            "Git author",
            "missing user.name or user.email; configure both before `shu sync init`",
        ))
    }
}

/// Confirm that the catalog's root, or its nearest existing parent, is writable.
fn check_root(catalog: &Catalog) -> Result<Check> {
    let root = root_path(catalog)?;
    let existing = existing_ancestor(&root).context("repository root has no existing ancestor")?;
    let metadata = fs::metadata(existing)?;
    if metadata.permissions().readonly() {
        Ok(Check::fail(
            "repository root",
            format!("{} is read-only", existing.display()),
        ))
    } else {
        Ok(Check::pass(
            "repository root",
            format!("{} (will be created when needed)", root.display()),
        ))
    }
}

/// Return the closest existing path from a possibly-not-yet-created target.
fn existing_ancestor(path: &Path) -> Option<&Path> {
    path.ancestors().find(|ancestor| ancestor.exists())
}

/// Inspect the optional in-catalog Git sync configuration.
fn check_sync(catalog: Option<&Catalog>, check_source: bool) -> Result<Check> {
    let Some(sync) = catalog.and_then(|catalog| catalog.sync.as_ref()) else {
        return Ok(Check::skip(
            "catalog source",
            "no [sync] configuration in shu.toml",
        ));
    };
    if !check_source {
        return Ok(Check::pass(
            "catalog source",
            format!(
                "configured: {} (run `shu doctor --check-source` to verify)",
                sync.remote
            ),
        ));
    }
    check_sync_checkout(catalog.expect("sync has a catalog"), sync)
}

/// Confirm that the configured persistent checkout is clean and its remote is reachable.
fn check_sync_checkout(catalog: &Catalog, sync: &Sync) -> Result<Check> {
    let checkout = root_path(catalog)?.join(crate::identity::normalize_identity(&sync.remote)?);
    if !git::is_repo(&checkout) {
        return Ok(Check::fail(
            "catalog source",
            format!(
                "checkout missing: {}; run `shu restore {}`",
                checkout.display(),
                sync.remote
            ),
        ));
    }
    if !git::is_clean(&checkout)? {
        return Ok(Check::fail(
            "catalog source",
            format!("checkout has local changes: {}", checkout.display()),
        ));
    }
    if let Err(error) = git::output(
        &checkout,
        ["ls-remote", "--exit-code", "origin", &sync.r#ref],
    ) {
        return Ok(Check::fail(
            "catalog source",
            format!("remote is not reachable on {}: {error}", sync.r#ref),
        ));
    }
    Ok(Check::pass(
        "catalog source",
        format!("reachable and clean: {}", checkout.display()),
    ))
}

/// Render diagnostics in either human-readable or machine-readable form.
fn render(cli: &Cli, checks: &[Check]) -> Result<()> {
    if cli.json {
        let output = DoctorOutput {
            schema_version: 1,
            checks,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }
    for check in checks {
        let marker = match check.status {
            CheckStatus::Pass => "✓",
            CheckStatus::Fail => "✗",
            CheckStatus::Skip => "-",
        };
        let line = format!("{}: {}", check.name, check.detail);
        match check.status {
            CheckStatus::Pass => ui::success(line),
            CheckStatus::Fail => println!("{} {line}", ui::failure_marker()),
            CheckStatus::Skip => println!("{marker} {line}"),
        }
    }
    Ok(())
}
