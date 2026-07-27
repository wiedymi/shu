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
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::{
    catalog,
    cli::{Cli, PickArgs},
    locations,
    model::Repo,
};

const MAX_VISIBLE_RESULTS: usize = 12;

/// Interactively select a present local repository with Shu's built-in fuzzy picker.
pub fn pick(cli: &Cli, args: &PickArgs) -> Result<()> {
    let (_, catalog) = catalog::load_or_initialize(cli)?;
    let candidates = catalog::filtered(&catalog, &args.filter)
        .filter_map(|repo| candidate(cli, &catalog, repo).transpose())
        .collect::<Result<Vec<_>>>()?;
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
    state: String,
    tags: String,
    path: PathBuf,
}

/// Convert a valid local catalog entry into a searchable picker candidate.
fn candidate(cli: &Cli, catalog: &crate::model::Catalog, repo: &Repo) -> Result<Option<Candidate>> {
    let Some(path) = locations::present_path(cli, catalog, repo)? else {
        return Ok(None);
    };
    Ok(Some(Candidate {
        identity: repo.source.clone(),
        state: repo.state.to_string(),
        tags: if repo.tags.is_empty() {
            "-".into()
        } else {
            repo.tags.join(",")
        },
        path,
    }))
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
            fuzzy_score(&candidate.identity, query).map(|score| (score, candidate.clone()))
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|(score, candidate)| (Reverse(*score), candidate.identity.clone()));
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
    for (index, character) in candidate.chars().flat_map(char::to_lowercase).enumerate() {
        if character != wanted {
            continue;
        }
        score += 10;
        if previous_index.is_some_and(|previous| index == previous + 1) {
            score += 8;
        }
        if index == 0
            || matches!(
                candidate.as_bytes().get(index.saturating_sub(1)),
                Some(b'/' | b'-' | b'_' | b'.')
            )
        {
            score += 4;
        }
        previous_index = Some(index);
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

    /// Redraw the compact picker UI while keeping output bounded to the terminal.
    fn render(&mut self, query: &str, candidates: &[Candidate], selected: usize) -> Result<()> {
        let visible = usize::from(terminal::size()?.1)
            .saturating_sub(4)
            .min(MAX_VISIBLE_RESULTS);
        execute!(self.stderr, MoveTo(0, 0), Clear(ClearType::All))?;
        writeln!(
            self.stderr,
            "Shu repositories  type to search · ↑↓ select · Enter open · Esc cancel"
        )?;
        writeln!(self.stderr, "› {query}")?;
        if candidates.is_empty() {
            writeln!(self.stderr, "  No matching local repositories")?;
        }
        for (index, candidate) in candidates.iter().take(visible).enumerate() {
            let marker = if index == selected { "›" } else { " " };
            writeln!(
                self.stderr,
                "{marker} {:<36} {:<10} {}",
                candidate.identity, candidate.state, candidate.tags
            )?;
        }
        self.stderr.flush()?;
        Ok(())
    }
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
}
