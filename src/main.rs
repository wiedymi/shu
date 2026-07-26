use std::{
    collections::HashSet,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(
    name = "shu",
    version,
    about = "A tiny, declarative, agent-friendly repository library"
)]
struct Cli {
    #[arg(long, global = true)]
    catalog: Option<PathBuf>,
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    non_interactive: bool,
    #[arg(long, global = true)]
    yes: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
struct AddArgs {
    source: String,
    #[arg(long, value_enum, default_value_t = Lifecycle::Active)]
    state: Lifecycle,
    #[arg(long)]
    tag: Vec<String>,
    #[arg(long)]
    note: Option<String>,
}

#[derive(Args)]
struct ScanArgs {
    #[arg(default_value = ".")]
    directory: PathBuf,
    #[arg(long)]
    add: bool,
}

#[derive(Args, Default)]
struct FilterArgs {
    #[arg(long)]
    tag: Option<String>,
    #[arg(long, value_enum)]
    state: Option<Lifecycle>,
}

#[derive(Args)]
struct RestoreArgs {
    source: Option<String>,
    #[arg(long)]
    file: Option<PathBuf>,
    #[arg(long)]
    git_ref: Option<String>,
    #[command(flatten)]
    filter: FilterArgs,
}

#[derive(Args)]
struct EnsureArgs {
    selector: String,
    #[arg(long)]
    path_only: bool,
}

#[derive(Args)]
struct SelectorArgs {
    selector: String,
}

#[derive(Args)]
struct ListArgs {
    #[command(flatten)]
    filter: FilterArgs,
    #[arg(long)]
    missing: bool,
    /// A duration such as 180d. Shows candidates only; it never changes lifecycle state.
    #[arg(long, num_args = 0..=1, default_missing_value = "180d")]
    stale: Option<String>,
}

#[derive(Args)]
struct StateArgs {
    selector: String,
    state: Lifecycle,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Lifecycle {
    Active,
    Parked,
    Reference,
    Archived,
}

impl std::fmt::Display for Lifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Active => "active",
                Self::Parked => "parked",
                Self::Reference => "reference",
                Self::Archived => "archived",
            }
        )
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Catalog {
    version: u32,
    #[serde(default = "default_root")]
    root: String,
    #[serde(default)]
    repos: Vec<Repo>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Repo {
    source: String,
    #[serde(default = "default_state")]
    state: Lifecycle,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Origin {
    source: String,
    file: Option<String>,
    git_ref: Option<String>,
}

#[derive(Debug, Serialize)]
struct ListOutput {
    schema_version: u32,
    repositories: Vec<RepoOutput>,
}

#[derive(Debug, Serialize)]
struct RepoOutput {
    identity: String,
    name: String,
    path: String,
    declared_state: Lifecycle,
    observed_state: String,
    tags: Vec<String>,
    note: Option<String>,
}

fn default_root() -> String {
    "~/Code".into()
}
fn default_state() -> Lifecycle {
    Lifecycle::Active
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Init => init(&cli),
        Commands::Add(args) => add(&cli, args),
        Commands::Scan(args) => scan(&cli, args),
        Commands::Status(filter) => status(&cli, filter),
        Commands::Restore(args) => restore(&cli, args),
        Commands::Update => update(&cli),
        Commands::Ensure(args) => ensure(&cli, args),
        Commands::Path(args) => path_command(&cli, args),
        Commands::List(args) => list(&cli, args),
        Commands::State(args) => change_state(&cli, &args.selector, args.state),
        Commands::Archive(args) => change_state(&cli, &args.selector, Lifecycle::Archived),
        Commands::Forget(args) => forget(&cli, args),
    }
}

fn dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("dev", "wiedymi", "shu")
        .ok_or_else(|| anyhow!("could not determine Shu configuration directory"))
}
fn catalog_path(cli: &Cli) -> Result<PathBuf> {
    Ok(cli
        .catalog
        .clone()
        .unwrap_or_else(|| dirs().unwrap().config_dir().join("shu.toml")))
}
fn origin_path(cli: &Cli) -> Result<PathBuf> {
    if let Some(catalog) = &cli.catalog {
        return Ok(catalog.with_extension("origin.json"));
    }
    Ok(dirs()?.config_dir().join("origin.json"))
}
fn state_dir() -> Result<PathBuf> {
    let project = dirs()?;
    Ok(project
        .state_dir()
        .unwrap_or_else(|| project.data_local_dir())
        .to_path_buf())
}

fn init(cli: &Cli) -> Result<()> {
    let path = catalog_path(cli)?;
    if path.exists() {
        bail!(
            "catalog already exists at {}; use --catalog to choose another path",
            path.display()
        );
    }
    let catalog = Catalog {
        version: 1,
        root: default_root(),
        repos: vec![],
    };
    save_catalog(&path, &catalog)?;
    println!("Initialized Shu catalog at {}", path.display());
    Ok(())
}

fn load_catalog(cli: &Cli) -> Result<(PathBuf, Catalog)> {
    let path = catalog_path(cli)?;
    let data = fs::read_to_string(&path).with_context(|| {
        format!(
            "could not read catalog {}; run `shu init` first",
            path.display()
        )
    })?;
    let catalog: Catalog =
        toml::from_str(&data).with_context(|| format!("invalid catalog {}", path.display()))?;
    if catalog.version != 1 {
        bail!("unsupported catalog version {}", catalog.version);
    }
    Ok((path, catalog))
}

fn save_catalog(path: &Path, catalog: &Catalog) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("catalog path has no parent"))?;
    fs::create_dir_all(parent)?;
    let text = toml::to_string_pretty(catalog)?;
    fs::write(path, text).with_context(|| format!("could not write catalog {}", path.display()))
}

fn add(cli: &Cli, args: &AddArgs) -> Result<()> {
    let (path, mut catalog) = load_catalog(cli)?;
    let source = source_from_argument(&args.source)?;
    let identity = normalize_identity(&source)?;
    if catalog
        .repos
        .iter()
        .any(|r| normalize_identity(&r.source).ok().as_deref() == Some(&identity))
    {
        bail!("{} is already in the catalog", identity);
    }
    catalog.repos.push(Repo {
        source: identity.clone(),
        state: args.state,
        tags: unique(args.tag.clone()),
        note: args.note.clone(),
    });
    save_catalog(&path, &catalog)?;
    println!("Added {identity}");
    Ok(())
}

fn source_from_argument(value: &str) -> Result<String> {
    if value == "." || Path::new(value).exists() {
        let path = if value == "." {
            std::env::current_dir()?
        } else {
            PathBuf::from(value)
        };
        let remote = git_output(&path, ["remote", "get-url", "origin"])?;
        return Ok(remote.trim().to_owned());
    }
    Ok(value.to_owned())
}

fn scan(cli: &Cli, args: &ScanArgs) -> Result<()> {
    let found = discover_repos(&args.directory)?;
    if args.add {
        let (path, mut catalog) = load_catalog(cli)?;
        let mut known: HashSet<String> = catalog
            .repos
            .iter()
            .filter_map(|r| normalize_identity(&r.source).ok())
            .collect();
        let mut added = 0;
        for (_, identity) in &found {
            if known.insert(identity.clone()) {
                catalog.repos.push(Repo {
                    source: identity.clone(),
                    state: Lifecycle::Active,
                    tags: vec![],
                    note: None,
                });
                added += 1;
            }
        }
        save_catalog(&path, &catalog)?;
        println!("Added {added} repository entries to {}", path.display());
    } else if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &found
                    .iter()
                    .map(|(path, source)| serde_json::json!({"path": path, "identity": source}))
                    .collect::<Vec<_>>()
            )?
        );
    } else {
        for (path, identity) in found {
            println!("{identity}\t{}", path.display());
        }
    }
    Ok(())
}

fn discover_repos(root: &Path) -> Result<Vec<(PathBuf, String)>> {
    if !root.exists() {
        bail!("scan directory does not exist: {}", root.display());
    }
    let mut found = vec![];
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".git")
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_dir() || !entry.path().join(".git").exists() {
            continue;
        }
        if let Ok(remote) = git_output(entry.path(), ["remote", "get-url", "origin"])
            && let Ok(identity) = normalize_identity(remote.trim())
        {
            found.push((entry.path().to_path_buf(), identity));
        }
    }
    found.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(found)
}

fn status(cli: &Cli, filter: &FilterArgs) -> Result<()> {
    let (_, catalog) = load_catalog(cli)?;
    let repos = filtered(&catalog, filter).collect::<Vec<_>>();
    if cli.json {
        return print_json(&catalog, repos);
    }
    println!("Catalog: {}", catalog_path(cli)?.display());
    println!("Root:    {}\n", root_path(&catalog)?.display());
    let mut current: Option<Lifecycle> = None;
    for repo in repos {
        if current != Some(repo.state) {
            current = Some(repo.state);
            println!("{}", repo.state.to_string().to_uppercase());
        }
        let observed = observed_state(&catalog, repo)?;
        println!("  {:<18} {}", repo_name(repo), observed);
    }
    let root = root_path(&catalog)?;
    let uncatalogued = if root.exists() {
        discover_repos(&root)?
            .into_iter()
            .filter(|(_, id)| {
                !catalog
                    .repos
                    .iter()
                    .any(|r| normalize_identity(&r.source).ok().as_deref() == Some(id))
            })
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    if !uncatalogued.is_empty() {
        println!("\nUNCATALOGUED");
        for (path, id) in uncatalogued {
            println!("  {:<18} {}", id, path.display());
        }
    }
    Ok(())
}

fn restore(cli: &Cli, args: &RestoreArgs) -> Result<()> {
    if let Some(source) = &args.source {
        let content =
            resolve_catalog_source(source, args.file.as_deref(), args.git_ref.as_deref())?;
        let catalog: Catalog =
            toml::from_str(&content).context("resolved catalog is not valid TOML")?;
        if catalog.version != 1 {
            bail!("unsupported catalog version {}", catalog.version);
        }
        let target = catalog_path(cli)?;
        save_catalog(&target, &catalog)?;
        let origin = Origin {
            source: source.clone(),
            file: args.file.as_ref().map(|p| p.display().to_string()),
            git_ref: args.git_ref.clone(),
        };
        let origin_file = origin_path(cli)?;
        fs::create_dir_all(origin_file.parent().unwrap())?;
        fs::write(origin_file, serde_json::to_vec_pretty(&origin)?)?;
        eprintln!("Using catalog from {source}");
    }
    let (_, catalog) = load_catalog(cli)?;
    let repos = filtered(&catalog, &args.filter).collect::<Vec<_>>();
    if args.source.is_some() && !cli.yes && !cli.non_interactive {
        eprint!(
            "Catalog contains {} repositories. Continue? [Y/n] ",
            repos.len()
        );
        io::stderr().flush()?;
        let mut response = String::new();
        io::stdin().read_line(&mut response)?;
        if !response.trim().is_empty()
            && !response.trim().eq_ignore_ascii_case("y")
            && !response.trim().eq_ignore_ascii_case("yes")
        {
            println!("No changes made.");
            return Ok(());
        }
    }
    let root = root_path(&catalog)?;
    fs::create_dir_all(&root)?;
    let mut failures = 0;
    for repo in repos {
        let target = repo_path(&catalog, repo)?;
        if is_git_repo(&target) {
            println!("✓ {} already present", repo_name(repo));
        } else if target.exists() {
            eprintln!(
                "! {} exists but is not a valid Git repository: {}",
                repo_name(repo),
                target.display()
            );
            failures += 1;
        } else {
            eprintln!("↓ cloning {}", repo.source);
            if let Err(error) = git_clone(&repo.source, &target) {
                eprintln!("! {}: {error:#}", repo_name(repo));
                failures += 1;
            }
        }
    }
    if failures > 0 {
        bail!("restore completed with {failures} conflict(s) or clone failure(s)");
    }
    Ok(())
}

fn update(cli: &Cli) -> Result<()> {
    let origin: Origin = serde_json::from_slice(
        &fs::read(origin_path(cli)?)
            .context("no saved catalog source; run `shu restore <source>` first")?,
    )?;
    restore(
        cli,
        &RestoreArgs {
            source: Some(origin.source),
            file: origin.file.map(PathBuf::from),
            git_ref: origin.git_ref,
            filter: FilterArgs::default(),
        },
    )
}

fn ensure(cli: &Cli, args: &EnsureArgs) -> Result<()> {
    let (_, catalog) = load_catalog(cli)?;
    let repo = select_repo(&catalog, &args.selector)?;
    let target = repo_path(&catalog, repo)?;
    if !is_git_repo(&target) {
        if target.exists() {
            bail!(
                "target path exists but is not a valid Git repository: {}",
                target.display()
            );
        }
        eprintln!("↓ cloning {}", repo.source);
        git_clone(&repo.source, &target)?;
    }
    let absolute_target = absolute(&target)?;
    if args.path_only {
        println!("{}", absolute_target.display());
    } else {
        println!(
            "Ensured {} at {}",
            repo_name(repo),
            absolute_target.display()
        );
    }
    Ok(())
}

fn path_command(cli: &Cli, args: &SelectorArgs) -> Result<()> {
    let (_, catalog) = load_catalog(cli)?;
    let repo = select_repo(&catalog, &args.selector)?;
    let target = repo_path(&catalog, repo)?;
    if !is_git_repo(&target) {
        bail!("repository is missing locally: {}", target.display());
    }
    println!("{}", absolute(&target)?.display());
    Ok(())
}

fn list(cli: &Cli, args: &ListArgs) -> Result<()> {
    let (_, catalog) = load_catalog(cli)?;
    let max_age = args.stale.as_deref().map(parse_duration).transpose()?;
    let repos = filtered(&catalog, &args.filter)
        .filter(|repo| {
            let path = repo_path(&catalog, repo).ok();
            (!args.missing || path.as_ref().is_some_and(|p| !is_git_repo(p)))
                && max_age.is_none_or(|age| path.is_some_and(|p| stale(&p, age)))
        })
        .collect::<Vec<_>>();
    if cli.json {
        return print_json(&catalog, repos);
    }
    for repo in repos {
        println!(
            "{:<24} {:<10} {}",
            normalize_identity(&repo.source)?,
            repo.state,
            observed_state(&catalog, repo)?
        );
    }
    Ok(())
}

fn change_state(cli: &Cli, selector: &str, state: Lifecycle) -> Result<()> {
    let (path, mut catalog) = load_catalog(cli)?;
    let index = select_index(&catalog, selector)?;
    catalog.repos[index].state = state;
    let name = repo_name(&catalog.repos[index]).to_string();
    save_catalog(&path, &catalog)?;
    println!("Set {name} to {state}");
    Ok(())
}

fn forget(cli: &Cli, args: &SelectorArgs) -> Result<()> {
    let (path, mut catalog) = load_catalog(cli)?;
    let index = select_index(&catalog, &args.selector)?;
    let removed = catalog.repos.remove(index);
    save_catalog(&path, &catalog)?;
    println!(
        "Removed {} from the catalog. Local repository was not deleted.",
        normalize_identity(&removed.source)?
    );
    Ok(())
}

fn filtered<'a>(catalog: &'a Catalog, filter: &'a FilterArgs) -> impl Iterator<Item = &'a Repo> {
    catalog.repos.iter().filter(move |repo| {
        filter.state.is_none_or(|state| repo.state == state)
            && filter
                .tag
                .as_ref()
                .is_none_or(|tag| repo.tags.iter().any(|t| t == tag))
    })
}

fn select_repo<'a>(catalog: &'a Catalog, selector: &str) -> Result<&'a Repo> {
    Ok(&catalog.repos[select_index(catalog, selector)?])
}
fn select_index(catalog: &Catalog, selector: &str) -> Result<usize> {
    let selector = if Path::new(selector).exists() {
        normalize_identity(&source_from_argument(selector)?)?
    } else {
        selector
            .trim()
            .trim_end_matches(".git")
            .trim_matches('/')
            .to_owned()
    };
    let matches = catalog
        .repos
        .iter()
        .enumerate()
        .filter_map(|(i, repo)| {
            let id = normalize_identity(&repo.source).ok()?;
            (id == selector || id.ends_with(&format!("/{selector}")) || repo_name(repo) == selector)
                .then_some(i)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => bail!("repository not found in catalog: {selector}"),
        _ => bail!("ambiguous repository selector: {selector}"),
    }
}

fn root_path(catalog: &Catalog) -> Result<PathBuf> {
    expand_home(&catalog.root)
}
fn repo_path(catalog: &Catalog, repo: &Repo) -> Result<PathBuf> {
    Ok(root_path(catalog)?.join(normalize_identity(&repo.source)?))
}
fn repo_name(repo: &Repo) -> &str {
    repo.source
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .next()
        .unwrap_or(&repo.source)
}
fn observed_state(catalog: &Catalog, repo: &Repo) -> Result<String> {
    let path = repo_path(catalog, repo)?;
    Ok(if is_git_repo(&path) {
        if stale(&path, Duration::from_secs(180 * 86400)) {
            "present (stale candidate)".into()
        } else {
            "present".into()
        }
    } else if path.exists() {
        "invalid".into()
    } else {
        "missing".into()
    })
}

fn normalize_identity(input: &str) -> Result<String> {
    let raw = input.trim().trim_end_matches('/').trim_end_matches(".git");
    let (host, path) = if let Some(rest) = raw.strip_prefix("git@") {
        let (host, path) = rest
            .split_once(':')
            .ok_or_else(|| anyhow!("invalid SSH Git URL: {input}"))?;
        (host.to_owned(), path.to_owned())
    } else if raw.contains("://") {
        let url = url::Url::parse(raw).with_context(|| format!("invalid URL: {input}"))?;
        (
            url.host_str()
                .ok_or_else(|| anyhow!("Git URL has no host: {input}"))?
                .to_owned(),
            url.path().trim_matches('/').to_owned(),
        )
    } else {
        let pieces = raw.trim_matches('/').split('/').collect::<Vec<_>>();
        if pieces.len() < 3 {
            bail!("repository identity must be host/namespace/repository: {input}");
        }
        (pieces[0].to_owned(), pieces[1..].join("/"))
    };
    let path = path.trim_matches('/').trim_end_matches(".git");
    if host.is_empty()
        || path.split('/').filter(|s| !s.is_empty()).count() < 2
        || path.contains("..")
    {
        bail!("invalid repository identity: {input}");
    }
    Ok(format!("{}/{}", host.to_ascii_lowercase(), path))
}

fn expand_home(value: &str) -> Result<PathBuf> {
    if value == "~" || value.starts_with("~/") || value.starts_with("~\\") {
        let home = directories::BaseDirs::new()
            .ok_or_else(|| anyhow!("could not determine home directory"))?
            .home_dir()
            .to_path_buf();
        return Ok(home.join(value[2..].replace('/', std::path::MAIN_SEPARATOR_STR)));
    }
    Ok(PathBuf::from(value))
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}
fn is_git_repo(path: &Path) -> bool {
    git_output(path, ["rev-parse", "--is-inside-work-tree"]).is_ok()
}
fn git_output<const N: usize>(dir: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .with_context(|| "could not run git; ensure Git is installed")?;
    if !output.status.success() {
        bail!(
            "git command failed in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
fn git_clone(identity: &str, target: &Path) -> Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("target has no parent"))?;
    fs::create_dir_all(parent)?;
    let remote = format!("https://{}.git", normalize_identity(identity)?);
    let status = Command::new("git")
        .args(["clone", "--", &remote])
        .arg(target)
        .stdin(Stdio::null())
        .status()
        .context("could not run git clone")?;
    if !status.success() {
        bail!("git clone failed for {identity}");
    }
    Ok(())
}

fn resolve_catalog_source(
    source: &str,
    file: Option<&Path>,
    git_ref: Option<&str>,
) -> Result<String> {
    if Path::new(source).is_file() {
        return fs::read_to_string(source)
            .with_context(|| format!("could not read catalog source {source}"));
    }
    if source.starts_with("http://") || source.starts_with("https://") {
        if source.contains("gist.github.com/") {
            return fetch_gist(source, file);
        }
        if source.ends_with(".toml") {
            return http_get(source);
        }
    }
    let identity = normalize_identity(source)?;
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    let key = format!("{:x}", hasher.finalize());
    let cache = state_dir()?.join("catalogs").join(&key[..16]);
    if !cache.exists() {
        fs::create_dir_all(cache.parent().unwrap())?;
        let remote = if source.contains("://") || source.starts_with("git@") {
            source.to_owned()
        } else {
            format!("https://{}.git", identity)
        };
        let mut command = Command::new("git");
        command.args(["clone", "--depth", "1"]);
        if let Some(reference) = git_ref {
            command.args(["--branch", reference]);
        }
        let status = command
            .arg(remote)
            .arg(&cache)
            .status()
            .context("could not clone catalog repository")?;
        if !status.success() {
            bail!("could not clone catalog repository {source}");
        }
    } else {
        let status = Command::new("git")
            .arg("-C")
            .arg(&cache)
            .args(["fetch", "--depth", "1", "origin"])
            .status()?;
        if !status.success() {
            bail!("could not refresh catalog repository {source}");
        }
        // This is Shu-owned cache state, never a user repository. Resetting it makes
        // `shu update` reflect the fetched catalog without touching local projects.
        let status = Command::new("git")
            .arg("-C")
            .arg(&cache)
            .args(["reset", "--hard", "FETCH_HEAD"])
            .status()?;
        if !status.success() {
            bail!("could not update cached catalog repository {source}");
        }
    }
    let filename = file.unwrap_or_else(|| Path::new("shu.toml"));
    let target = cache.join(filename);
    fs::read_to_string(&target).with_context(|| {
        format!(
            "catalog file {} was not found in {source}",
            filename.display()
        )
    })
}

fn http_get(url: &str) -> Result<String> {
    reqwest::blocking::Client::builder()
        .user_agent("shu/0.1")
        .build()?
        .get(url)
        .send()?
        .error_for_status()?
        .text()
        .context("could not read catalog response")
}
fn fetch_gist(source: &str, file: Option<&Path>) -> Result<String> {
    let id = source
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .ok_or_else(|| anyhow!("invalid Gist URL"))?;
    let api = format!("https://api.github.com/gists/{id}");
    let value: serde_json::Value = reqwest::blocking::Client::builder()
        .user_agent("shu/0.1")
        .build()?
        .get(api)
        .send()?
        .error_for_status()?
        .json()?;
    let wanted = file
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .unwrap_or("shu.toml");
    value["files"][wanted]["content"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("Gist does not contain {wanted}"))
}

fn print_json(catalog: &Catalog, repos: Vec<&Repo>) -> Result<()> {
    let repositories = repos
        .into_iter()
        .map(|repo| {
            let path = repo_path(catalog, repo)?;
            Ok(RepoOutput {
                identity: normalize_identity(&repo.source)?,
                name: repo_name(repo).to_owned(),
                path: absolute(&path)?.display().to_string(),
                declared_state: repo.state,
                observed_state: observed_state(catalog, repo)?,
                tags: repo.tags.clone(),
                note: repo.note.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&ListOutput {
            schema_version: 1,
            repositories
        })?
    );
    Ok(())
}
fn stale(path: &Path, max_age: Duration) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| SystemTime::now().duration_since(m).ok())
        .is_some_and(|age| age > max_age)
}
fn parse_duration(value: &str) -> Result<Duration> {
    let days = value
        .trim_end_matches('d')
        .parse::<u64>()
        .context("stale duration must be like 180d")?;
    Ok(Duration::from_secs(days * 86400))
}
fn unique(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|i| seen.insert(i.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_common_remote_forms() {
        for value in [
            "https://github.com/wiedymi/shu.git",
            "git@github.com:wiedymi/shu.git",
            "ssh://git@github.com/wiedymi/shu.git",
            "github.com/wiedymi/shu",
        ] {
            assert_eq!(normalize_identity(value).unwrap(), "github.com/wiedymi/shu");
        }
    }
    #[test]
    fn rejects_short_identity() {
        assert!(normalize_identity("wiedymi/shu").is_err());
    }
    #[test]
    fn parses_stale_duration() {
        assert_eq!(
            parse_duration("180d").unwrap(),
            Duration::from_secs(180 * 86400)
        );
    }
}
