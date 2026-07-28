//! Interactive selection for restoring a newly supplied catalog source.

use std::io::{Write, stderr};

use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::{locations, model::Catalog};

/// Select repositories from a source catalog by collection or individually.
pub fn select(catalog: &Catalog, candidates: Vec<usize>) -> Result<Option<Vec<usize>>> {
    let mut terminal = Terminal::enter()?;
    if !catalog.collections.is_empty() && choose_mode(&mut terminal)? == Some(Mode::Collections) {
        let names = catalog.collections.keys().cloned().collect::<Vec<_>>();
        let Some(selected) = choose_many(&mut terminal, "Select collections", &names, &[])? else {
            return Ok(None);
        };
        return Ok(Some(
            candidates
                .into_iter()
                .filter(|index| {
                    selected.iter().any(|selected| {
                        catalog.collections[&names[*selected]]
                            .tags
                            .iter()
                            .all(|tag| catalog.repos[*index].tags.contains(tag))
                    })
                })
                .collect(),
        ));
    }
    let rows = candidates
        .iter()
        .map(|index| catalog.repos[*index].source.clone())
        .collect::<Vec<_>>();
    let selected_by_default = candidates
        .iter()
        .enumerate()
        .filter_map(|(row, index)| {
            locations::present_path(catalog, &catalog.repos[*index])
                .ok()
                .flatten()
                .is_none()
                .then_some(row)
        })
        .collect::<Vec<_>>();
    let Some(selected) = choose_many(
        &mut terminal,
        "Select repositories",
        &rows,
        &selected_by_default,
    )?
    else {
        return Ok(None);
    };
    Ok(Some(
        selected.into_iter().map(|row| candidates[row]).collect(),
    ))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Collections,
    Repositories,
}

fn choose_mode(terminal: &mut Terminal) -> Result<Option<Mode>> {
    let mut selected = 0usize;
    loop {
        terminal.render(
            "Restore mode",
            &["Collections".into(), "Repositories".into()],
            &[],
            selected,
            "↑↓ navigate  enter continue  esc cancel",
        )?;
        match key()? {
            Some(KeyCode::Esc) => return Ok(None),
            Some(KeyCode::Up) => selected = selected.saturating_sub(1),
            Some(KeyCode::Down) => selected = (selected + 1).min(1),
            Some(KeyCode::Enter) => {
                return Ok(Some(if selected == 0 {
                    Mode::Collections
                } else {
                    Mode::Repositories
                }));
            }
            _ => {}
        }
    }
}

fn choose_many(
    terminal: &mut Terminal,
    title: &str,
    rows: &[String],
    initial: &[usize],
) -> Result<Option<Vec<usize>>> {
    let mut selected = initial.to_vec();
    let mut cursor = 0usize;
    loop {
        terminal.render(
            title,
            rows,
            &selected,
            cursor,
            "↑↓ navigate  space toggle  enter continue  esc cancel",
        )?;
        match key()? {
            Some(KeyCode::Esc) => return Ok(None),
            Some(KeyCode::Up) => cursor = cursor.saturating_sub(1),
            Some(KeyCode::Down) => cursor = (cursor + 1).min(rows.len().saturating_sub(1)),
            Some(KeyCode::Char(' ')) => {
                if let Some(index) = selected.iter().position(|value| *value == cursor) {
                    selected.remove(index);
                } else {
                    selected.push(cursor);
                }
            }
            Some(KeyCode::Enter) => return Ok(Some(selected)),
            _ => {}
        }
    }
}

fn key() -> Result<Option<KeyCode>> {
    loop {
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => return Ok(Some(key.code)),
            _ => {}
        }
    }
}

struct Terminal {
    stderr: std::io::Stderr,
}
impl Terminal {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let mut stderr = stderr();
        execute!(stderr, EnterAlternateScreen, Hide)?;
        Ok(Self { stderr })
    }
    fn render(
        &mut self,
        title: &str,
        rows: &[String],
        selected: &[usize],
        cursor: usize,
        help: &str,
    ) -> Result<()> {
        execute!(self.stderr, MoveTo(0, 0), Clear(ClearType::All))?;
        writeln!(self.stderr, "{title}\r")?;
        for (index, row) in rows.iter().enumerate() {
            writeln!(
                self.stderr,
                "{} [{}] {row}\r",
                if index == cursor { "›" } else { " " },
                if selected.contains(&index) { "x" } else { " " }
            )?;
        }
        writeln!(self.stderr, "\r{} selected  ·  {help}\r", selected.len())?;
        self.stderr.flush()?;
        Ok(())
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = execute!(self.stderr, Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}
