//! Small, consistent terminal presentation helpers.
//!
//! Shu keeps command output plain enough to read in logs and scripts while
//! adding color only when the receiving stream is an interactive terminal.

use std::io::{IsTerminal, stderr, stdout};

use anyhow::Error;
use crossterm::style::{Color, Stylize};

/// Render a concise error with its immediate underlying cause when available.
pub fn render_error(error: &Error) {
    eprintln!(
        "{} {error}",
        label("error", Color::Red, stderr().is_terminal())
    );
    let mut causes = error.chain();
    let _ = causes.next();
    if let Some(cause) = causes.next()
        && cause.to_string() != error.to_string()
    {
        eprintln!(
            "  {} {cause}",
            label("cause", Color::DarkGrey, stderr().is_terminal())
        );
    }
    eprintln!(
        "  {} Run `shu --help` for a command overview.",
        label("help", Color::Cyan, stderr().is_terminal())
    );
}

/// Return a success marker suitable for human-readable command output.
pub fn success_marker() -> String {
    label("✓", Color::Green, stdout().is_terminal())
}

/// Return an attention marker suitable for human-readable command output.
pub fn warning_marker() -> String {
    label("!", Color::Yellow, stdout().is_terminal())
}

/// Return a failure marker suitable for human-readable command output.
pub fn failure_marker() -> String {
    label("✗", Color::Red, stdout().is_terminal())
}

/// Add terminal color only when the output can render it correctly.
fn label(text: &str, color: Color, color_enabled: bool) -> String {
    if color_enabled {
        format!("{}", text.with(color).bold())
    } else {
        text.to_owned()
    }
}
