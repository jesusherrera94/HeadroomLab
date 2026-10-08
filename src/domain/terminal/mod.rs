mod build;
mod screen;
mod shell;

pub use crate::config::GIT_FOR_WINDOWS_URL;
pub use build::{BuildKind, BuildStatus, BuildUnavailable, build_command};
pub use screen::{CellStyle, Rgb, TerminalCell, TerminalPalette, TerminalSnapshot, indexed_color};
pub use shell::{find_git_bash, git_bash_candidates, shell_for};

pub const SCROLLBACK_LINES: usize = 10_000;

pub const MIN_COLS: u16 = 2;
pub const MIN_ROWS: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Linux,
    Windows,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellChoice {
    pub program: String,
    pub args: Vec<String>,
}

impl ShellChoice {
    pub fn new(program: impl Into<String>, args: &[&str]) -> Self {
        Self {
            program: program.into(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
        }
    }

    pub fn label(&self) -> String {
        let base = self
            .program
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&self.program);
        base.strip_suffix(".exe").unwrap_or(base).to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

pub fn grid_size(width: f32, height: f32, cell_width: f32, cell_height: f32) -> (u16, u16) {
    let cols = safe_div(width, cell_width).max(MIN_COLS as usize);
    let rows = safe_div(height, cell_height).max(MIN_ROWS as usize);
    (
        cols.min(u16::MAX as usize) as u16,
        rows.min(u16::MAX as usize) as u16,
    )
}

fn safe_div(available: f32, unit: f32) -> usize {
    if !unit.is_finite() || unit <= 0.0 || !available.is_finite() || available <= 0.0 {
        return 0;
    }
    (available / unit).floor() as usize
}

#[cfg(test)]
mod tests;
