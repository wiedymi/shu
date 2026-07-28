//! Shu's built-in fuzzy repository picker.

use std::{
    cmp::Reverse,
    io::{Write, stderr},
    path::PathBuf,
};

use anyhow::{Result, bail};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Attribute, SetAttribute},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::{
    catalog,
    cli::{Cli, PickArgs},
    locations,
    model::Repo,
};

/// Interactively select a present local repository with Shu's built-in fuzzy picker.
pub fn pick(cli: &Cli, args: &PickArgs) -> Result<()> {
    let (_, catalog) = catalog::load_or_initialize(cli)?;
    let candidates = catalog::filtered(&catalog, &args.filter)
        .map(|repo| candidates(&catalog, repo))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
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
            println!(
                "Selected {}\n{}",
                selected.identity,
                selected.path.display()
            );
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Candidate {
    identity: String,
    location: LocationKind,
    path: PathBuf,
}

/// A location is either an independent clone or a Git-linked worktree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocationKind {
    Clone,
    Worktree,
}

impl LocationKind {
    fn label(self) -> &'static str {
        match self {
            Self::Clone => "clone",
            Self::Worktree => "worktree",
        }
    }
}

/// Convert each present clone and Git worktree into a searchable picker candidate.
fn candidates(catalog: &crate::model::Catalog, repo: &Repo) -> Result<Vec<Candidate>> {
    let clones = locations::present_paths(catalog, repo)?;
    locations::pickable_paths(catalog, repo).map(|paths| {
        paths
            .into_iter()
            .map(|path| Candidate {
                identity: repo.source.clone(),
                location: if clones.contains(&path) {
                    LocationKind::Clone
                } else {
                    LocationKind::Worktree
                },
                path,
            })
            .collect()
    })
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
            Event::Key(key) => match key {
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

    /// Redraw the result list above a single bottom-aligned search prompt.
    fn render(&mut self, query: &str, candidates: &[Candidate], selected: usize) -> Result<()> {
        let (width, height) = terminal_size()?;
        let visible = usize::from(height.saturating_sub(1));
        execute!(self.stderr, MoveTo(0, 0), Clear(ClearType::All))?;
        let first_visible = selected.saturating_sub(visible.saturating_sub(1));
        let visible_candidates = candidates
            .iter()
            .skip(first_visible)
            .take(visible)
            .collect::<Vec<_>>();
        let first_row = height
            .saturating_sub(1)
            .saturating_sub(visible_candidates.len() as u16);
        for (index, candidate) in visible_candidates.into_iter().enumerate() {
            let row = format!(
                "[{}] {}",
                candidate.location.label(),
                candidate.path.display()
            );
            execute!(
                self.stderr,
                MoveTo(0, first_row + index as u16),
                Clear(ClearType::CurrentLine)
            )?;
            if first_visible + index == selected {
                execute!(self.stderr, SetAttribute(Attribute::Reverse))?;
                write!(
                    self.stderr,
                    "› {}",
                    fit_to_width(&row, width.saturating_sub(2))
                )?;
                execute!(self.stderr, SetAttribute(Attribute::Reset))?;
            } else {
                write!(
                    self.stderr,
                    "  {}",
                    fit_to_width(&row, width.saturating_sub(2))
                )?;
            }
        }
        execute!(
            self.stderr,
            MoveTo(0, height.saturating_sub(1)),
            Clear(ClearType::CurrentLine),
            SetAttribute(Attribute::Reverse)
        )?;
        write!(
            self.stderr,
            "› {}",
            tail_to_width(query, width.saturating_sub(2))
        )?;
        execute!(self.stderr, SetAttribute(Attribute::Reset))?;
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

/// Show the active end of a long query, where newly typed text appears.
fn tail_to_width(text: &str, width: u16) -> String {
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
    format!(
        "…{}",
        text.chars().skip(length - width + 1).collect::<String>()
    )
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
    fn picker_rows_stay_on_one_line_and_keep_the_active_query_visible() {
        assert_eq!(fit_to_width("/repositories/example", 8), "/reposi…");
        assert_eq!(tail_to_width("repository", 5), "…tory");
    }

    #[test]
    fn locations_have_clear_distinct_labels() {
        assert_eq!(LocationKind::Clone.label(), "clone");
        assert_eq!(LocationKind::Worktree.label(), "worktree");
    }
}
