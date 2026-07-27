//! Read-only environment checks for setting up and maintaining Shu.

use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::{
    catalog,
    cli::{Cli, DoctorArgs},
    model::{Catalog, Origin},
    paths::root_path,
    sources,
};

/// Check the local Shu setup and report actionable failures.
///
/// The default checks never access the network or modify repositories. Passing
/// `--check-source` also resolves the remembered catalog source; Git-backed
/// sources may refresh Shu's private catalog cache while doing so.
pub fn doctor(cli: &Cli, args: &DoctorArgs) -> Result<()> {
    let mut checks = Vec::new();
    checks.push(check_git());

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

    checks.push(check_origin(cli, args.check_source)?);
    render(cli, &checks)?;
    if checks.iter().any(|check| check.status == CheckStatus::Fail) {
        bail!("Shu setup has failing checks")
    }
    Ok(())
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

/// Inspect the optional remembered source and resolve it on explicit request.
fn check_origin(cli: &Cli, check_source: bool) -> Result<Check> {
    let path = catalog::origin_path(cli)?;
    if !path.exists() {
        return Ok(Check::skip(
            "catalog source",
            "no remembered source; use `shu restore <source>` to configure one",
        ));
    }
    let origin: Origin = serde_json::from_slice(
        &fs::read(&path).with_context(|| format!("could not read {}", path.display()))?,
    )
    .with_context(|| format!("invalid source metadata {}", path.display()))?;
    if !check_source {
        return Ok(Check::pass(
            "catalog source",
            format!(
                "configured: {} (run `shu doctor --check-source` to verify)",
                origin.source
            ),
        ));
    }
    match sources::resolve(
        &origin.source,
        origin.file.as_deref().map(Path::new),
        origin.git_ref.as_deref(),
    ) {
        Ok(content) => match toml::from_str::<Catalog>(&content) {
            Ok(_) => Ok(Check::pass(
                "catalog source",
                format!("reachable: {}", origin.source),
            )),
            Err(error) => Ok(Check::fail(
                "catalog source",
                format!("invalid TOML: {error}"),
            )),
        },
        Err(error) => Ok(Check::fail(
            "catalog source",
            format!("unreachable: {error:#}"),
        )),
    }
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
        println!("{marker} {}: {}", check.name, check.detail);
    }
    Ok(())
}
