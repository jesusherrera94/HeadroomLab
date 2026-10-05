use std::path::{Path, PathBuf};

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

pub fn shell_for(
    platform: Platform,
    env: &dyn Fn(&str) -> Option<String>,
    exists: &dyn Fn(&Path) -> bool,
) -> ShellChoice {
    match platform {
        Platform::MacOs => unix_shell(env, "/bin/zsh"),
        Platform::Linux => unix_shell(env, "/bin/bash"),
        Platform::Windows => match find_git_bash(env, exists) {
            Some(bash) => ShellChoice::new(bash.to_string_lossy().into_owned(), &["--login", "-i"]),
            None => ShellChoice::new("powershell.exe", &["-NoLogo"]),
        },
    }
}

fn unix_shell(env: &dyn Fn(&str) -> Option<String>, fallback: &str) -> ShellChoice {
    let program = env("SHELL")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string());
    ShellChoice::new(program, &[])
}

pub fn find_git_bash(
    env: &dyn Fn(&str) -> Option<String>,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    git_bash_candidates(env).into_iter().find(|p| exists(p))
}

pub fn git_bash_candidates(env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(path) = env("PATH") {
        for dir in path.split(';').filter(|d| !d.trim().is_empty()) {
            candidates.push(windows_join(dir, "bash.exe"));
        }
    }

    for (var, suffix) in [
        ("ProgramFiles", r"Git\bin\bash.exe"),
        ("ProgramFiles(x86)", r"Git\bin\bash.exe"),
        ("LOCALAPPDATA", r"Programs\Git\bin\bash.exe"),
    ] {
        if let Some(root) = env(var).filter(|r| !r.trim().is_empty()) {
            candidates.push(windows_join(&root, suffix));
        }
    }

    candidates
}

fn windows_join(root: &str, rest: &str) -> PathBuf {
    let root = root.trim().trim_end_matches(['\\', '/']);
    PathBuf::from(format!(r"{root}\{rest}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalPalette {
    pub named: [Rgb; 16],
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
}

pub fn indexed_color(index: u8, palette: &TerminalPalette) -> Rgb {
    match index {
        0..=15 => palette.named[index as usize],
        16..=231 => {
            const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
            let i = index as usize - 16;
            Rgb::new(LEVELS[i / 36], LEVELS[(i % 36) / 6], LEVELS[i % 6])
        }
        232..=255 => {
            let level = 8 + (index as u16 - 232) * 10;
            let level = level.min(255) as u8;
            Rgb::new(level, level, level)
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CellStyle {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCell {
    pub c: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub style: CellStyle,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSnapshot {
    pub cols: u16,
    pub rows: u16,
    pub cells: Vec<TerminalCell>,
    pub cursor: Option<(u16, u16)>,
    pub display_offset: usize,
    pub history_len: usize,
}

impl TerminalSnapshot {
    pub fn row(&self, row: u16) -> &[TerminalCell] {
        let cols = self.cols as usize;
        let start = row as usize * cols;
        self.cells.get(start..start + cols).unwrap_or(&[])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildKind {
    Dylib,
    Firmware,
}

impl BuildKind {
    pub fn target(self) -> &'static str {
        match self {
            BuildKind::Dylib => "dylib",
            BuildKind::Firmware => "",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BuildKind::Dylib => "Build & Run",
            BuildKind::Firmware => "Compile",
        }
    }

    pub fn command_line(self) -> String {
        match self {
            BuildKind::Dylib => "make dylib".to_string(),
            BuildKind::Firmware => "make".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildUnavailable {
    NeedsGitBash,
}

pub use crate::config::GIT_FOR_WINDOWS_URL;

pub fn build_command(
    kind: BuildKind,
    platform: Platform,
    env: &dyn Fn(&str) -> Option<String>,
    exists: &dyn Fn(&Path) -> bool,
) -> Result<ShellChoice, BuildUnavailable> {
    match platform {
        Platform::MacOs | Platform::Linux => {
            let target = kind.target();
            let args: &[&str] = if target.is_empty() { &[] } else { &[target] };
            Ok(ShellChoice::new("make", args))
        }
        Platform::Windows => {
            let bash = find_git_bash(env, exists).ok_or(BuildUnavailable::NeedsGitBash)?;
            Ok(ShellChoice::new(
                bash.to_string_lossy().into_owned(),
                &["-lc", &kind.command_line()],
            ))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildStatus {
    Running,
    Succeeded,
    Failed(i32),
}

impl BuildStatus {
    pub fn summary(self, kind: BuildKind) -> String {
        match self {
            BuildStatus::Running => format!("{}…", kind.label()),
            BuildStatus::Succeeded => format!("{} succeeded", kind.label()),
            BuildStatus::Failed(code) => format!("{} failed ({code})", kind.label()),
        }
    }
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
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    fn exists_of(paths: &[&str]) -> impl Fn(&Path) -> bool + use<> {
        let set: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        move |p: &Path| set.iter().any(|known| known == p)
    }

    fn nothing_exists(_: &Path) -> bool {
        false
    }

    #[test]
    fn unix_honours_shell_and_falls_back_per_platform() {
        let fish = env_of(&[("SHELL", "/opt/homebrew/bin/fish")]);
        assert_eq!(
            shell_for(Platform::MacOs, &fish, &nothing_exists),
            ShellChoice::new("/opt/homebrew/bin/fish", &[])
        );

        let empty = env_of(&[]);
        assert_eq!(
            shell_for(Platform::MacOs, &empty, &nothing_exists),
            ShellChoice::new("/bin/zsh", &[])
        );
        assert_eq!(
            shell_for(Platform::Linux, &empty, &nothing_exists),
            ShellChoice::new("/bin/bash", &[])
        );
    }

    #[test]
    fn a_blank_shell_variable_is_treated_as_unset() {
        let blank = env_of(&[("SHELL", "   ")]);
        assert_eq!(
            shell_for(Platform::Linux, &blank, &nothing_exists),
            ShellChoice::new("/bin/bash", &[])
        );
    }

    #[test]
    fn windows_prefers_git_bash_when_it_exists() {
        let env = env_of(&[("ProgramFiles", r"C:\Program Files")]);
        let exists = exists_of(&[r"C:\Program Files\Git\bin\bash.exe"]);
        assert_eq!(
            shell_for(Platform::Windows, &env, &exists),
            ShellChoice::new(r"C:\Program Files\Git\bin\bash.exe", &["--login", "-i"])
        );
    }

    #[test]
    fn windows_falls_back_to_powershell_so_the_terminal_still_works() {
        let env = env_of(&[("ProgramFiles", r"C:\Program Files")]);
        assert_eq!(
            shell_for(Platform::Windows, &env, &nothing_exists),
            ShellChoice::new("powershell.exe", &["-NoLogo"])
        );
    }

    #[test]
    fn git_bash_on_path_wins_over_the_installer_locations() {
        let env = env_of(&[
            ("PATH", r"C:\tools;C:\Users\me\scoop\shims"),
            ("ProgramFiles", r"C:\Program Files"),
        ]);
        let exists = exists_of(&[
            r"C:\Users\me\scoop\shims\bash.exe",
            r"C:\Program Files\Git\bin\bash.exe",
        ]);
        assert_eq!(
            find_git_bash(&env, &exists),
            Some(PathBuf::from(r"C:\Users\me\scoop\shims\bash.exe"))
        );
    }

    #[test]
    fn the_discovery_order_covers_every_documented_location() {
        let env = env_of(&[
            ("PATH", r"C:\tools"),
            ("ProgramFiles", r"C:\Program Files"),
            ("ProgramFiles(x86)", r"C:\Program Files (x86)"),
            ("LOCALAPPDATA", r"C:\Users\me\AppData\Local"),
        ]);
        assert_eq!(
            git_bash_candidates(&env),
            vec![
                PathBuf::from(r"C:\tools\bash.exe"),
                PathBuf::from(r"C:\Program Files\Git\bin\bash.exe"),
                PathBuf::from(r"C:\Program Files (x86)\Git\bin\bash.exe"),
                PathBuf::from(r"C:\Users\me\AppData\Local\Programs\Git\bin\bash.exe"),
            ]
        );
    }

    #[test]
    fn an_absent_environment_yields_no_candidates() {
        assert!(git_bash_candidates(&env_of(&[])).is_empty());
        assert_eq!(find_git_bash(&env_of(&[]), &nothing_exists), None);
    }

    #[test]
    fn unix_builds_run_make_directly_so_the_exit_code_is_the_builds() {
        let env = env_of(&[]);
        assert_eq!(
            build_command(BuildKind::Dylib, Platform::MacOs, &env, &nothing_exists),
            Ok(ShellChoice::new("make", &["dylib"]))
        );
        assert_eq!(
            build_command(BuildKind::Firmware, Platform::Linux, &env, &nothing_exists),
            Ok(ShellChoice::new("make", &[]))
        );
    }

    #[test]
    fn windows_builds_go_through_git_bash() {
        let env = env_of(&[("ProgramFiles", r"C:\Program Files")]);
        let exists = exists_of(&[r"C:\Program Files\Git\bin\bash.exe"]);
        assert_eq!(
            build_command(BuildKind::Dylib, Platform::Windows, &env, &exists),
            Ok(ShellChoice::new(
                r"C:\Program Files\Git\bin\bash.exe",
                &["-lc", "make dylib"]
            ))
        );
    }

    #[test]
    fn windows_without_git_bash_cannot_build_but_says_why() {
        let env = env_of(&[]);
        assert_eq!(
            build_command(BuildKind::Dylib, Platform::Windows, &env, &nothing_exists),
            Err(BuildUnavailable::NeedsGitBash)
        );
    }

    #[test]
    fn a_shell_is_labelled_by_its_basename() {
        assert_eq!(ShellChoice::new("/bin/zsh", &[]).label(), "zsh");
        assert_eq!(
            ShellChoice::new(r"C:\Program Files\Git\bin\bash.exe", &[]).label(),
            "bash"
        );
        assert_eq!(
            ShellChoice::new("powershell.exe", &[]).label(),
            "powershell"
        );
    }

    #[test]
    fn build_status_reads_the_same_everywhere_it_is_shown() {
        assert_eq!(
            BuildStatus::Running.summary(BuildKind::Dylib),
            "Build & Run…"
        );
        assert_eq!(
            BuildStatus::Succeeded.summary(BuildKind::Firmware),
            "Compile succeeded"
        );
        assert_eq!(
            BuildStatus::Failed(2).summary(BuildKind::Dylib),
            "Build & Run failed (2)"
        );
    }

    #[test]
    fn grid_size_floors_to_whole_cells() {
        assert_eq!(grid_size(800.0, 300.0, 8.0, 16.0), (100, 18));
        // 803/8 = 100.375 → 100 columns, not 101.
        assert_eq!(grid_size(803.0, 300.0, 8.0, 16.0), (100, 18));
    }

    #[test]
    fn a_collapsed_panel_never_produces_a_zero_sized_pty() {
        assert_eq!(grid_size(0.0, 0.0, 8.0, 16.0), (MIN_COLS, MIN_ROWS));
        // The panel is measured before the font is, so both can be nonsense.
        assert_eq!(grid_size(800.0, 300.0, 0.0, 0.0), (MIN_COLS, MIN_ROWS));
        assert_eq!(grid_size(f32::NAN, 300.0, 8.0, 16.0), (MIN_COLS, 18));
    }
}
