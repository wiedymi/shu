//! Shu is a small, declarative library for local Git repositories.
//!
//! The binary is the primary public interface. Run `shu --help` for an
//! overview, or generate implementation documentation with:
//!
//! ```text
//! cargo doc --no-deps --document-private-items --open
//! ```

#![warn(missing_docs)]

/// Catalog storage, selection, and configuration helpers.
mod catalog;
/// Command-line parsing and help text.
mod cli;
/// Implementations for user-facing CLI commands.
mod commands;
/// Safe wrappers around the installed Git executable.
mod git;
/// Hashing helpers for cache keys and release verification.
mod hash;
/// Shared HTTPS client configuration and download helpers.
mod http;
/// Git remote and repository-identity normalization.
mod identity;
/// Local paths observed on this machine and canonical managed destinations.
mod locations;
/// Serializable catalog data structures.
mod model;
/// Filesystem path derivation helpers.
mod paths;
/// Loading catalogs from files, URLs, Gists, and Git repositories.
mod sources;
/// Consistent terminal output for people and scripts.
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

/// Run the CLI and render any error as a concise diagnostic.
fn main() {
    if let Err(error) = run() {
        ui::render_error(&error);
        std::process::exit(1);
    }
}

/// Parse arguments and dispatch the selected command implementation.
fn run() -> Result<()> {
    let cli = Cli::parse();
    validate_json(&cli)?;
    match &cli.command {
        None => commands::pick(&cli, &Default::default()),
        Some(Commands::Init) => commands::init(&cli),
        Some(Commands::Add(args)) => commands::add(&cli, args),
        Some(Commands::New(args)) => commands::new(&cli, args),
        Some(Commands::Edit(args)) => commands::edit(&cli, args),
        Some(Commands::Scan(args)) => commands::scan(&cli, args),
        Some(Commands::Doctor(args)) => commands::doctor(&cli, args),
        Some(Commands::Status(filter)) => commands::status(&cli, filter),
        Some(Commands::Restore(args)) => commands::restore(&cli, args),
        Some(Commands::Sync(args)) => commands::sync(&cli, args),
        Some(Commands::Update(args)) => commands::update(&cli, args),
        Some(Commands::Upgrade(args)) => commands::upgrade(args),
        Some(Commands::Ensure(args)) => commands::ensure(&cli, args),
        Some(Commands::Path(args)) => commands::path_command(&cli, args),
        Some(Commands::Locations(args)) => commands::locations_command(&cli, args),
        Some(Commands::List(args)) => commands::list(&cli, args),
        Some(Commands::State(args)) => commands::change_state(&cli, &args.selector, args.state),
        Some(Commands::Archive(args)) => {
            commands::change_state(&cli, &args.selector, model::Lifecycle::Archived)
        }
        Some(Commands::Forget(args)) => commands::forget(&cli, args),
        Some(Commands::Pick(args)) => commands::pick(&cli, args),
        Some(Commands::Shell(args)) => commands::shell(&args.command),
    }
}

/// Reject JSON where the command has no stable machine-readable response.
fn validate_json(cli: &Cli) -> Result<()> {
    if !cli.json {
        return Ok(());
    }
    let supported = match &cli.command {
        Some(Commands::Doctor(_)) | Some(Commands::Status(_)) | Some(Commands::List(_)) => true,
        Some(Commands::Scan(args)) => !args.add,
        _ => false,
    };
    if supported {
        Ok(())
    } else {
        anyhow::bail!(
            "--json is supported by `list`, `status`, `doctor`, and `scan` without `--add`"
        );
    }
}
