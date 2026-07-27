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
/// Git remote and repository-identity normalization.
mod identity;
/// Serializable catalog data structures.
mod model;
/// Filesystem path derivation helpers.
mod paths;
/// Loading catalogs from files, URLs, Gists, and Git repositories.
mod sources;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

/// Run the CLI and render any error as a concise diagnostic.
fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

/// Parse arguments and dispatch the selected command implementation.
fn run() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        None => commands::pick(&cli, &Default::default()),
        Some(Commands::Init) => commands::init(&cli),
        Some(Commands::Add(args)) => commands::add(&cli, args),
        Some(Commands::Scan(args)) => commands::scan(&cli, args),
        Some(Commands::Doctor(args)) => commands::doctor(&cli, args),
        Some(Commands::Status(filter)) => commands::status(&cli, filter),
        Some(Commands::Restore(args)) => commands::restore(&cli, args),
        Some(Commands::Update) => commands::update(&cli),
        Some(Commands::Upgrade(args)) => commands::upgrade(args),
        Some(Commands::Ensure(args)) => commands::ensure(&cli, args),
        Some(Commands::Path(args)) => commands::path_command(&cli, args),
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
