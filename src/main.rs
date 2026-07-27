mod catalog;
mod cli;
mod commands;
mod git;
mod identity;
mod model;
mod paths;
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
