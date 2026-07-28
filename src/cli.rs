//! Clap definitions for the human and automation-facing command line.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::model::Lifecycle;

/// Parsed command-line arguments for Shu.
#[derive(Parser)]
#[command(
    name = "shu",
    version,
    about = "A tiny, declarative, agent-friendly repository library",
    long_about = "Shu keeps one readable catalog of Git repositories, restores missing clones into predictable paths, and lets people and agents resolve repositories reliably.",
    after_help = "Examples:\n  shu init\n  shu add github.com/example-org/api --tag backend\n  shu clone github.com/example-org/api\n  shu new github.com/example-org/service\n  shu new github.com/example-org/private-service --github --private\n  shu add . --migrate --dry-run\n  shu locations api\n  shu restore github.com/your-account/repository-library\n  shu path api"
)]
pub struct Cli {
    /// Use a specific catalog rather than the active catalog.
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Use a specific catalog file"
    )]
    pub catalog: Option<PathBuf>,
    /// Emit structured output for scripts and agents.
    #[arg(
        long,
        global = true,
        help = "Emit stable, versioned JSON where supported"
    )]
    pub json: bool,
    /// Fail instead of displaying a confirmation prompt.
    #[arg(
        long,
        global = true,
        help = "Never prompt; fail if confirmation is required"
    )]
    pub non_interactive: bool,
    /// Accept safe confirmation prompts.
    #[arg(long, global = true, help = "Accept confirmation prompts")]
    pub yes: bool,
    /// The operation to run; omitting it opens the repository picker.
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// All supported Shu commands.
#[derive(Subcommand)]
pub enum Commands {
    /// Create an empty catalog at the active catalog location.
    Init,
    /// Add a repository to the catalog, cloning remote identities when needed.
    #[command(visible_alias = "clone")]
    Add(AddArgs),
    /// Create a new local Git repository in Shu's managed library.
    New(NewArgs),
    /// Change repository metadata such as its lifecycle state or note.
    Edit(EditArgs),
    /// Discover Git repositories below a directory.
    Scan(ScanArgs),
    /// Check whether Git, the catalog, repository root, and source setup are ready to use.
    Doctor(DoctorArgs),
    /// Compare the catalog with repositories available on this machine.
    Status(FilterArgs),
    /// Load a catalog source if provided, then clone missing repositories.
    Restore(RestoreArgs),
    /// Initialize or push the catalog Git checkout configured in shu.toml.
    Sync(SyncArgs),
    /// Refresh the configured Git catalog and restore any newly missing repositories.
    #[command(
        after_help = "To edit a repository's metadata, use `shu edit <repository> --state <state>` or `shu edit <repository> --note <text>`."
    )]
    Update(UpdateArgs),
    /// Download and install the latest verified Shu binary from GitHub Releases.
    Upgrade(UpgradeArgs),
    /// Ensure a catalogued repository exists locally and print its path.
    Ensure(EnsureArgs),
    /// Print the local path of an existing catalogued repository.
    Path(SelectorArgs),
    /// Show known local clone paths or choose the preferred clone for a repository.
    Locations(LocationsArgs),
    /// List catalogued repositories with optional filters.
    List(ListArgs),
    /// Change only a repository's declared lifecycle state.
    State(StateArgs),
    /// Mark a repository as archived without moving or deleting it.
    Archive(SelectorArgs),
    /// Remove a repository from the catalog without deleting its local clone.
    Forget(SelectorArgs),
    /// Interactively select a present local repository with Shu's fuzzy picker.
    Pick(PickArgs),
    /// Print shell integration that makes bare `shu` navigate into a selected repository.
    Shell(ShellArgs),
}

/// Arguments for `shu add` and its `shu clone` alias.
#[derive(Args)]
pub struct AddArgs {
    /// A repository identity, Git URL, or `.` for the current repository.
    ///
    /// Remote identities are cloned into Shu's managed root. Local clones are
    /// recorded without moving them; add `--migrate` to move one into that root.
    #[arg(value_name = "REPOSITORY")]
    pub source: String,
    /// Lifecycle state to record in the catalog.
    #[arg(long, value_enum, default_value_t = Lifecycle::Active)]
    pub state: Lifecycle,
    /// A repeatable label used for filtering and grouping.
    #[arg(long)]
    pub tag: Vec<String>,
    /// Optional human-readable context for the repository.
    #[arg(long)]
    pub note: Option<String>,
    /// Move a clean local working tree into Shu's canonical repository layout.
    #[arg(long)]
    pub migrate: bool,
    /// Preview a migration without moving files or changing the catalog.
    #[arg(long, requires = "migrate")]
    pub dry_run: bool,
}

/// Arguments for `shu new`.
#[derive(Args)]
pub struct NewArgs {
    /// Full repository identity, such as github.com/account/project.
    #[arg(value_name = "REPOSITORY")]
    pub source: String,
    /// Lifecycle state to record in the catalog.
    #[arg(long, value_enum, default_value_t = Lifecycle::Active)]
    pub state: Lifecycle,
    /// A repeatable label used for filtering and grouping.
    #[arg(long)]
    pub tag: Vec<String>,
    /// Optional human-readable context for the repository.
    #[arg(long)]
    pub note: Option<String>,
    /// Create the matching GitHub repository through the authenticated gh CLI.
    #[arg(long)]
    pub github: bool,
    /// Create the GitHub repository as private; requires --github.
    #[arg(long, requires = "github")]
    pub private: bool,
}

/// Arguments for `shu sync`.
#[derive(Args, Default)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub command: Option<SyncCommand>,
}

/// One explicit sync workflow.
#[derive(Subcommand)]
pub enum SyncCommand {
    /// Create, publish, and activate a dedicated catalog repository.
    Init(SyncInitArgs),
}

/// Arguments for `shu sync init`.
#[derive(Args)]
pub struct SyncInitArgs {
    /// Git remote or full repository identity for the catalog repository.
    #[arg(value_name = "REPOSITORY")]
    pub source: String,
    /// Create the matching GitHub repository through the authenticated gh CLI.
    #[arg(long)]
    pub github: bool,
    /// Create the GitHub repository as private; requires --github.
    #[arg(long, requires = "github")]
    pub private: bool,
}

/// Arguments for `shu edit`.
#[derive(Args)]
pub struct EditArgs {
    /// Full identity, unique suffix, repository name, or local repository path.
    pub selector: String,
    /// New lifecycle state to record.
    #[arg(long, value_enum)]
    pub state: Option<Lifecycle>,
    /// Replace the existing note with this human-readable context.
    #[arg(long, conflicts_with = "clear_note")]
    pub note: Option<String>,
    /// Remove the existing note.
    #[arg(long)]
    pub clear_note: bool,
}

/// Arguments for `shu scan`.
#[derive(Args)]
pub struct ScanArgs {
    /// Directory to recursively search for Git repositories.
    #[arg(default_value = ".")]
    pub directory: PathBuf,
    /// Add newly discovered repositories to the catalog.
    #[arg(long)]
    pub add: bool,
}

/// Arguments for `shu doctor`.
#[derive(Args, Default)]
pub struct DoctorArgs {
    /// Resolve the remembered catalog source to confirm it is currently reachable.
    #[arg(long)]
    pub check_source: bool,
    /// Verify that gh is installed and authenticated for `shu new --github`.
    #[arg(long)]
    pub check_github: bool,
}

/// Shared tag and lifecycle filters.
#[derive(Args, Default)]
pub struct FilterArgs {
    /// Include only repositories with this tag.
    #[arg(long)]
    pub tag: Option<String>,
    /// Include only repositories in this lifecycle state.
    #[arg(long, value_enum)]
    pub state: Option<Lifecycle>,
}

/// Arguments for `shu restore`.
#[derive(Args)]
pub struct RestoreArgs {
    /// Local TOML file, HTTPS URL, Gist URL, or Git repository containing a catalog.
    #[arg(value_name = "SOURCE")]
    pub source: Option<String>,
    /// Catalog path within a Git repository or Gist; defaults to `shu.toml`.
    #[arg(long)]
    pub file: Option<PathBuf>,
    /// Git branch or tag to use when the source is a Git repository.
    #[arg(long = "ref")]
    pub git_ref: Option<String>,
    /// Limit restoration to a tag or lifecycle state.
    #[command(flatten)]
    pub filter: FilterArgs,
}

/// Compatibility arguments that let `shu update` explain a common command mix-up.
#[derive(Args, Default)]
pub struct UpdateArgs {
    /// Repository selector accidentally supplied to the catalog refresh command.
    #[arg(value_name = "REPOSITORY", hide = true)]
    pub selector: Option<String>,
    /// Lifecycle state accidentally supplied to the catalog refresh command.
    #[arg(long, value_enum, hide = true)]
    pub state: Option<Lifecycle>,
    /// Repository note accidentally supplied to the catalog refresh command.
    #[arg(long, hide = true)]
    pub note: Option<String>,
}

/// Arguments for `shu upgrade`.
#[derive(Args, Default)]
pub struct UpgradeArgs {
    /// Release tag or version to install; defaults to the latest release.
    #[arg(long, value_name = "VERSION")]
    pub version: Option<String>,
}

/// Arguments for `shu ensure`.
#[derive(Args)]
pub struct EnsureArgs {
    /// Full identity, unique suffix, or unique repository name.
    pub selector: String,
    /// Print only the absolute path, with progress sent to stderr.
    #[arg(long)]
    pub path_only: bool,
}

/// A repository selector shared by path-oriented commands.
#[derive(Args)]
pub struct SelectorArgs {
    /// Full identity, unique suffix, or unique repository name.
    pub selector: String,
}

/// Arguments for `shu locations`.
#[derive(Args)]
pub struct LocationsArgs {
    /// Full identity, unique suffix, or unique repository name.
    pub selector: String,
    /// Make one already known local clone the preferred path for this repository.
    #[arg(long, value_name = "PATH")]
    pub primary: Option<PathBuf>,
}

/// Arguments for `shu list`.
#[derive(Args)]
pub struct ListArgs {
    /// Limit listed repositories by tag or lifecycle state.
    #[command(flatten)]
    pub filter: FilterArgs,
    /// Show only catalog entries missing from this machine.
    #[arg(long)]
    pub missing: bool,
    /// Show only repositories whose local activity is older than this duration, such as `180d`.
    #[arg(long, num_args = 0..=1, default_missing_value = "180d", value_name = "DURATION")]
    pub stale: Option<String>,
}

/// Arguments for `shu state`.
#[derive(Args)]
pub struct StateArgs {
    /// The repository to update.
    pub selector: String,
    /// The new declared lifecycle state.
    pub state: Lifecycle,
}

/// Arguments for `shu pick`.
#[derive(Args, Default)]
pub struct PickArgs {
    /// Limit candidates by tag or lifecycle state.
    #[command(flatten)]
    pub filter: FilterArgs,
    /// Pre-fill Shu's fuzzy-search input while keeping the picker interactive.
    #[arg(long, conflicts_with = "filter_query")]
    pub query: Option<String>,
    /// Select non-interactively using a fuzzy query; useful for scripts and integration tests.
    #[arg(long = "filter", value_name = "QUERY", conflicts_with = "query")]
    pub filter_query: Option<String>,
    /// Print exactly one absolute path on success and no decorative text.
    #[arg(long)]
    pub path_only: bool,
}

/// Arguments for `shu shell`.
#[derive(Args)]
pub struct ShellArgs {
    /// Shell-integration operation to perform.
    #[command(subcommand)]
    pub command: ShellCommands,
}

/// Supported shell-integration operations.
#[derive(Subcommand)]
pub enum ShellCommands {
    /// Install Shu's navigation wrapper in a shell startup file.
    Init(ShellInitArgs),
}

/// Arguments for `shu shell init`.
#[derive(Args)]
pub struct ShellInitArgs {
    /// Shell to integrate with.
    #[arg(value_enum)]
    pub shell: Shell,
    /// Print the integration instead of writing it to a startup file.
    #[arg(long, conflicts_with = "path")]
    pub print: bool,
    /// Install into this startup file instead of Shu's default for the selected shell.
    #[arg(long, value_name = "PATH", conflicts_with = "print")]
    pub path: Option<PathBuf>,
}

/// Shell syntaxes supported by Shu's navigation wrapper.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Shell {
    /// Bash shell syntax.
    Bash,
    /// Zsh shell syntax.
    Zsh,
    /// Fish shell syntax.
    Fish,
    /// PowerShell syntax.
    #[value(name = "powershell", alias = "pwsh")]
    Power,
    /// Nushell syntax.
    Nushell,
    /// Portable POSIX `sh` syntax for shells such as dash and ksh.
    Posix,
}
