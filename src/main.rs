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

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Init => commands::init(&cli),
        Commands::Add(args) => commands::add(&cli, args),
        Commands::Scan(args) => commands::scan(&cli, args),
        Commands::Status(filter) => commands::status(&cli, filter),
        Commands::Restore(args) => commands::restore(&cli, args),
        Commands::Update => commands::update(&cli),
        Commands::Ensure(args) => commands::ensure(&cli, args),
        Commands::Path(args) => commands::path_command(&cli, args),
        Commands::List(args) => commands::list(&cli, args),
        Commands::State(args) => commands::change_state(&cli, &args.selector, args.state),
        Commands::Archive(args) => {
            commands::change_state(&cli, &args.selector, model::Lifecycle::Archived)
        }
        Commands::Forget(args) => commands::forget(&cli, args),
    }
}
