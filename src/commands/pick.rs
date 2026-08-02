//! Shu's built-in fuzzy repository picker.

use std::{
    cmp::Reverse,
    io::{Write, stderr},
    path::PathBuf,
    thread,
};

use anyhow::{Result, anyhow, bail};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    style::{Color, Stylize},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::{
    catalog,
    cli::{Cli, PickArgs},
    git, locations,
    model::Repo,
};

/// Interactively select a present local repository with Shu's built-in fuzzy picker.
pub fn pick(cli: &Cli, args: &PickArgs) -> Result<()> {
    let (_, catalog) = catalog::load_or_initialize(cli)?;
    let candidates = all_candidates(&catalog, catalog::filtered(&catalog, &args.filter)?)?;
    if candidates.is_empty() {
        bail!(
            "no catalogued repositories are available locally. Run `shu status` to see expected or recorded paths; use `shu ensure <repository>` to clone one, or run `shu add .` from an existing clone to record it"
        );
    }

    let selected = if let Some(query) = &args.filter_query {
        ranked(&candidates, query).into_iter().next()
    } else {
        choose_interactively(candidates, args.query.as_deref().unwrap_or_default())?
    };

    if let Some(selected) = selected {
        if args.path_only {
            println!("{}", selected.path.display());
        } else {
            crate::ui::success(format!("Selected {}", selected.identity));
            crate::ui::detail("location", selected.path.display());
        }
    }
    Ok(())
}

/// Observe independent repositories concurrently without overwhelming the machine.
fn all_candidates(catalog: &crate::model::Catalog, repos: Vec<&Repo>) -> Result<Vec<Candidate>> {
    const MAX_WORKERS: usize = 8;

    if repos.is_empty() {
        return Ok(Vec::new());
    }
    let workers = thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(MAX_WORKERS)
        .min(repos.len());
    let chunk_size = repos.len().div_ceil(workers);
    thread::scope(|scope| {
        let handles = repos
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|repo| candidates(catalog, repo))
                        .collect::<Result<Vec<_>>>()
                })
            })
            .collect::<Vec<_>>();
        let mut observed = Vec::new();
        for handle in handles {
            observed.extend(
                handle
                    .join()
                    .map_err(|_| anyhow!("repository observation worker panicked"))??,
            );
        }
        Ok(observed.into_iter().flatten().collect())
    })
}

#[derive(Clone, Debug)]
struct Candidate {
    identity: String,
    location: LocationKind,
    branch: Option<String>,
    path: PathBuf,
}

/// A picker location is derived from catalog preference and Git worktree state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocationKind {
    Primary,
    Checkout,
    Worktree,
}

impl LocationKind {
    fn symbol(self) -> &'static str {
        match self {
            Self::Primary => "◆",
            Self::Checkout => "◇",
            Self::Worktree => "⎇",
        }
    }
}

/// Convert each present clone and Git worktree into a searchable picker candidate.
fn candidates(catalog: &crate::model::Catalog, repo: &Repo) -> Result<Vec<Candidate>> {
    let primary = locations::primary_path(catalog, repo)?;
    let mut clone_paths = locations::remembered_paths(catalog, repo)?;
    push_unique(&mut clone_paths, locations::managed_path(catalog, repo)?);

    let mut probes = clone_paths
        .iter()
        .cloned()
        .map(|path| (path, true))
        .collect::<Vec<_>>();
    if let Some(primary) = &primary
        && !clone_paths.contains(primary)
    {
        probes.push((primary.clone(), false));
    }

    let mut clones = Vec::new();
    let mut worktrees = Vec::new();
    let mut primary_is_present = false;
    for (path, is_clone) in probes {
        let Some(observed) = git::inspect_worktrees(&path)? else {
            continue;
        };
        if primary.as_ref() == Some(&path) {
            primary_is_present = true;
        }
        if is_clone {
            push_unique(&mut clones, path);
            for worktree in observed {
                if !worktrees
                    .iter()
                    .any(|known: &git::Worktree| known.path == worktree.path)
                {
                    worktrees.push(worktree);
                }
            }
        }
    }

    let primary = primary.filter(|_| primary_is_present);
    let mut candidates = Vec::new();
    if let Some(primary) = &primary {
        candidates.push(Candidate {
            identity: repo.source.clone(),
            location: LocationKind::Primary,
            branch: None,
            path: primary.clone(),
        });
    }
    for clone in clones {
        if primary.as_ref() != Some(&clone) {
            candidates.push(Candidate {
                identity: repo.source.clone(),
                location: LocationKind::Checkout,
                branch: None,
                path: clone,
            });
        }
    }
    for worktree in worktrees {
        if primary.as_ref() == Some(&worktree.path)
            || candidates
                .iter()
                .any(|candidate| candidate.path == worktree.path)
        {
            continue;
        }
        candidates.push(Candidate {
            identity: repo.source.clone(),
            location: LocationKind::Worktree,
            branch: worktree.branch,
            path: worktree.path,
        });
    }
    Ok(candidates)
}

/// Append one path unless the same location is already known.
fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

/// Run a raw-key terminal interface until the user selects a candidate or cancels.
fn choose_interactively(
    candidates: Vec<Candidate>,
    initial_query: &str,
) -> Result<Option<Candidate>> {
    let mut terminal = PickerTerminal::enter()?;
    let mut query = initial_query.to_owned();
    let mut selected = 0usize;
    loop {
        let matches = ranked(&candidates, &query);
        if selected >= matches.len() {
            selected = matches.len().saturating_sub(1);
        }
        terminal.render(&query, &matches, selected)?;
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key {
                KeyEvent {
                    code: KeyCode::Esc, ..
                }
                | KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers: KeyModifiers::CONTROL,
                    ..
                } => return Ok(None),
                KeyEvent {
                    code: KeyCode::Enter,
                    ..
                } => return Ok(matches.get(selected).cloned()),
                KeyEvent {
                    code: KeyCode::Up, ..
                } => selected = selected.saturating_sub(1),
                KeyEvent {
                    code: KeyCode::Down,
                    ..
                } => selected = (selected + 1).min(matches.len().saturating_sub(1)),
                KeyEvent {
                    code: KeyCode::Backspace,
                    ..
                } => {
                    query.pop();
                    selected = 0;
                }
                KeyEvent {
                    code: KeyCode::Char(character),
                    modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                    ..
                } => {
                    query.push(character);
                    selected = 0;
                }
                _ => {}
            },
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
}

/// Rank candidates using an in-order character match that favors contiguous matches.
fn ranked(candidates: &[Candidate], query: &str) -> Vec<Candidate> {
    let mut matches = candidates
        .iter()
        .filter_map(|candidate| {
            fuzzy_score(
                &format!("{} {}", candidate.identity, candidate.path.display()),
                query,
            )
            .map(|score| (score, candidate.clone()))
        })
        .collect::<Vec<_>>();
    // `sort_by_key` is stable, so equal fuzzy scores retain the primary-first
    // order supplied by `locations::pickable_paths`.
    matches.sort_by_key(|(score, _)| Reverse(*score));
    matches
        .into_iter()
        .map(|(_, candidate)| candidate)
        .collect()
}

/// Return a fuzzy score when each query character appears in order in a candidate.
fn fuzzy_score(candidate: &str, query: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    let mut score = 0;
    let mut previous_index = None;
    let mut query_chars = query.chars().flat_map(char::to_lowercase);
    let mut wanted = query_chars.next()?;
    let mut previous_character = None;
    for (index, character) in candidate.chars().flat_map(char::to_lowercase).enumerate() {
        if character != wanted {
            previous_character = Some(character);
            continue;
        }
        score += 10;
        if previous_index.is_some_and(|previous| index == previous + 1) {
            score += 8;
        }
        if index == 0 || matches!(previous_character, Some('/' | '-' | '_' | '.')) {
            score += 4;
        }
        previous_index = Some(index);
        previous_character = Some(character);
        match query_chars.next() {
            Some(next) => wanted = next,
            None => return Some(score),
        }
    }
    None
}

struct PickerTerminal {
    stderr: std::io::Stderr,
}

impl PickerTerminal {
    /// Enter the alternate screen and raw-key mode used by the picker.
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        // The selected path must travel over stdout so a shell wrapper can
        // capture it. Draw the interactive interface on stderr instead: shell
        // command substitution leaves that stream attached to the terminal.
        let mut stderr = stderr();
        execute!(stderr, EnterAlternateScreen, Hide)?;
        Ok(Self { stderr })
    }

    /// Redraw Shu's focused repository picker.
    fn render(&mut self, query: &str, candidates: &[Candidate], selected: usize) -> Result<()> {
        let (width, height) = terminal_size()?;
        let visible = usize::from(height.saturating_sub(6)) / 3;
        execute!(self.stderr, MoveTo(0, 0), Clear(ClearType::All))?;
        let first_visible = selected.saturating_sub(visible.saturating_sub(1));
        let visible_candidates = candidates
            .iter()
            .skip(first_visible)
            .take(visible)
            .collect::<Vec<_>>();
        // Raw mode disables the terminal's normal NL-to-CRLF conversion. Use
        // CRLF explicitly so every logical row starts in column zero instead
        // of continuing from the end of the preceding row.
        write!(
            self.stderr,
            "{}\r\n\r\n",
            "Pick a repository".with(Color::Cyan).bold()
        )?;
        write!(self.stderr, "{} ", " ".on(Color::Cyan))?;
        if query.is_empty() {
            write!(
                self.stderr,
                "{}\r\n\r\n",
                "Search repositories".with(Color::DarkGrey)
            )?;
        } else {
            write!(self.stderr, "{query}\r\n\r\n")?;
        }
        for (index, candidate) in visible_candidates.into_iter().enumerate() {
            let row = &candidate.identity;
            if first_visible + index == selected {
                write!(
                    self.stderr,
                    "{} {}  {}\r\n",
                    "›".with(Color::Cyan).bold(),
                    fit_to_width(row, width.saturating_sub(8)).bold(),
                    candidate.location.symbol().with(Color::Cyan).bold(),
                )?;
            } else {
                write!(
                    self.stderr,
                    "  {}  {}\r\n",
                    fit_to_width(row, width.saturating_sub(8)),
                    candidate.location.symbol().with(Color::Cyan),
                )?;
            }
            let branch = candidate
                .branch
                .as_ref()
                .map(|branch| format!("  {branch}"))
                .unwrap_or_default();
            write!(
                self.stderr,
                "  {}{}\r\n\r\n",
                fit_to_width(
                    &candidate.path.display().to_string(),
                    width.saturating_sub(4)
                )
                .with(Color::DarkGrey),
                branch.with(Color::DarkGrey),
            )?;
        }
        let count = candidates.len();
        write!(
            self.stderr,
            "{} {}   {} {}   {} {}\r\n",
            "◆".with(Color::Cyan),
            "primary".with(Color::DarkGrey),
            "◇".with(Color::Cyan),
            "checkout".with(Color::DarkGrey),
            "⎇".with(Color::Cyan),
            "worktree".with(Color::DarkGrey),
        )?;
        write!(
            self.stderr,
            "{}  ·  {}\r\n",
            format!("{count} matches").with(Color::DarkGrey),
            "↑↓ navigate  / filter  enter open  esc cancel".with(Color::DarkGrey),
        )?;
        self.stderr.flush()?;
        Ok(())
    }
}

/// Return a usable screen size even when a terminal reports a transient zero size.
fn terminal_size() -> Result<(u16, u16)> {
    Ok(match terminal::size()? {
        (0, _) | (_, 0) => (80, 24),
        size => size,
    })
}

/// Keep each row inside the terminal instead of letting long paths wrap.
fn fit_to_width(text: &str, width: u16) -> String {
    let width = usize::from(width);
    let length = text.chars().count();
    if length <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".to_owned();
    }
    format!("{}…", text.chars().take(width - 1).collect::<String>())
}

impl Drop for PickerTerminal {
    fn drop(&mut self) {
        let _ = execute!(self.stderr, Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matching_requires_order_and_rewards_contiguous_characters() {
        assert!(fuzzy_score("github.com/example-org/api", "goa").is_some());
        assert!(fuzzy_score("github.com/example-org/api", "pag").is_none());
        assert!(
            fuzzy_score("github.com/example-org/api", "api").unwrap()
                > fuzzy_score("github.com/example-org/api", "ai").unwrap()
        );
    }

    #[test]
    fn picker_rows_stay_on_one_line() {
        assert_eq!(fit_to_width("/repositories/example", 8), "/reposi…");
    }

    #[test]
    fn locations_have_clear_distinct_symbols() {
        assert_eq!(LocationKind::Primary.symbol(), "◆");
        assert_eq!(LocationKind::Checkout.symbol(), "◇");
        assert_eq!(LocationKind::Worktree.symbol(), "⎇");
    }
}
