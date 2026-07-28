//! Shared human-facing terminal components.
//!
//! Commands own facts and machine-readable output. This module owns the small,
//! terminal-aware visual language used for people: cyan actions, semantic
//! outcomes, dim supporting detail, and honest progress.

use std::io::{IsTerminal, Write, stderr, stdout};

use anyhow::Error;
use crossterm::style::{Color, Stylize};

/// Render a concise error with its immediate underlying cause and recovery hint.
pub fn render_error(error: &Error) {
    eprintln!("{} {error}", failure_marker());
    let mut causes = error.chain();
    let _ = causes.next();
    if let Some(cause) = causes.next()
        && cause.to_string() != error.to_string()
    {
        detail_to_stderr("cause", cause);
    }
    detail_to_stderr("help", "Run `shu --help` for a command overview.");
}

/// Print a compact command heading.
pub fn heading(title: impl std::fmt::Display) {
    println!("{} {title}", accent("shu"));
}

/// Print one operation that is starting or may take time.
pub fn action(message: impl std::fmt::Display) {
    println!("{} {message}", action_marker());
}

/// Print an operation that is currently waiting on external work.
pub fn working(message: impl std::fmt::Display) {
    eprintln!("{} {message}", working_marker());
}

/// Print a completed outcome.
pub fn success(message: impl std::fmt::Display) {
    println!("{} {message}", success_marker());
}

/// Print a warning or attention outcome.
pub fn warning(message: impl std::fmt::Display) {
    println!("{} {message}", warning_marker());
}

/// Print a warning on standard error when it accompanies a recoverable failure.
pub fn warning_to_stderr(message: impl std::fmt::Display) {
    eprintln!(
        "{} {message}",
        tone("!", Color::Yellow, stderr().is_terminal())
    );
}

/// Print one dim, aligned supporting value on standard output.
pub fn detail(label: &str, value: impl std::fmt::Display) {
    println!("  {} {value}", dim(format!("{label:<10}")));
}

/// Print one dim, aligned supporting value on standard error.
pub fn detail_to_stderr(label: &str, value: impl std::fmt::Display) {
    eprintln!("  {} {value}", dim(format!("{label:<10}")));
}

/// Return a success marker suitable for human-readable command output.
pub fn success_marker() -> String {
    tone("✓", Color::Green, stdout().is_terminal())
}

/// Return an attention marker suitable for human-readable command output.
pub fn warning_marker() -> String {
    tone("!", Color::Yellow, stdout().is_terminal())
}

/// Return a failure marker suitable for human-readable command output.
pub fn failure_marker() -> String {
    tone("×", Color::Red, stderr().is_terminal())
}

/// Return the cyan action marker used for commands and picker structure.
pub fn action_marker() -> String {
    tone("→", Color::Cyan, stdout().is_terminal())
}

/// Return the cyan working marker used when a total is not known yet.
pub fn working_marker() -> String {
    tone("…", Color::Cyan, stderr().is_terminal())
}

/// Apply the terminal's dim attribute only when it can be rendered.
pub fn dim(value: impl std::fmt::Display) -> String {
    let value = value.to_string();
    if stdout().is_terminal() {
        format!("{}", value.with(Color::DarkGrey))
    } else {
        value
    }
}

/// Apply Shu's cyan accent only when it can be rendered.
pub fn accent(value: impl std::fmt::Display) -> String {
    tone(&value.to_string(), Color::Cyan, stdout().is_terminal())
}

/// Draw byte-based download progress without inventing totals or percentages.
pub fn render_download_progress(downloaded: u64, total: Option<u64>) -> std::io::Result<()> {
    if !stderr().is_terminal() {
        return Ok(());
    }
    let detail = match total.filter(|total| *total > 0) {
        Some(total) => format!(
            "{}  {} / {}  {}%",
            progress_bar(downloaded, total),
            human_size(downloaded),
            human_size(total),
            u128::from(downloaded) * 100 / u128::from(total)
        ),
        None => human_size(downloaded),
    };
    // A carriage return alone leaves trailing cells from an earlier frame in
    // place on some terminals. Erase the entire line before drawing the next
    // byte-accurate frame.
    eprint!("\r\x1b[2K  {detail}");
    stderr().flush()
}

/// Render a fixed-width bar from actual byte counts for a sized transfer.
fn progress_bar(downloaded: u64, total: u64) -> String {
    const WIDTH: u64 = 16;
    let filled =
        (u128::from(downloaded.min(total)) * u128::from(WIDTH) / u128::from(total)) as usize;
    accent(format!(
        "[{}{}]",
        "█".repeat(filled),
        "·".repeat(WIDTH as usize - filled)
    ))
}

/// Format a byte count for compact human-facing progress.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn tone(text: &str, color: Color, enabled: bool) -> String {
    if enabled {
        format!("{}", text.with(color).bold())
    } else {
        text.to_owned()
    }
}
