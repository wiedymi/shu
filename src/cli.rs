use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::model::Lifecycle;

#[derive(Parser)]
#[command(
    name = "shu",
    version,
    about = "A tiny, declarative, agent-friendly repository library"
)]
pub struct Cli {
    #[arg(long, global = true)]
    pub catalog: Option<PathBuf>,
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true)]
    pub non_interactive: bool,
    #[arg(long, global = true)]
    pub yes: bool,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Init,
    Add(AddArgs),
    Scan(ScanArgs),
    Status(FilterArgs),
    Restore(RestoreArgs),
    Update,
    Ensure(EnsureArgs),
    Path(SelectorArgs),
    List(ListArgs),
    State(StateArgs),
    Archive(SelectorArgs),
    Forget(SelectorArgs),
}

#[derive(Args)]
pub struct AddArgs {
    pub source: String,
    #[arg(long, value_enum, default_value_t = Lifecycle::Active)]
    pub state: Lifecycle,
    #[arg(long)]
    pub tag: Vec<String>,
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Args)]
pub struct ScanArgs {
    #[arg(default_value = ".")]
    pub directory: PathBuf,
    #[arg(long)]
    pub add: bool,
}

#[derive(Args, Default)]
pub struct FilterArgs {
    #[arg(long)]
    pub tag: Option<String>,
    #[arg(long, value_enum)]
    pub state: Option<Lifecycle>,
}

#[derive(Args)]
pub struct RestoreArgs {
    pub source: Option<String>,
    #[arg(long)]
    pub file: Option<PathBuf>,
    #[arg(long)]
    pub git_ref: Option<String>,
    #[command(flatten)]
    pub filter: FilterArgs,
}

#[derive(Args)]
pub struct EnsureArgs {
    pub selector: String,
    #[arg(long)]
    pub path_only: bool,
}

#[derive(Args)]
pub struct SelectorArgs {
    pub selector: String,
}

#[derive(Args)]
pub struct ListArgs {
    #[command(flatten)]
    pub filter: FilterArgs,
    #[arg(long)]
    pub missing: bool,
    #[arg(long, num_args = 0..=1, default_missing_value = "180d")]
    pub stale: Option<String>,
}

#[derive(Args)]
pub struct StateArgs {
    pub selector: String,
    pub state: Lifecycle,
}
