use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::*;

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

#[test]
fn a_snapshot_row_past_the_grid_is_empty_rather_than_a_panic() {
    let cell = TerminalCell {
        c: 'x',
        fg: Rgb::new(0, 0, 0),
        bg: Rgb::new(0, 0, 0),
        style: CellStyle::default(),
        selected: false,
    };
    let snapshot = TerminalSnapshot {
        cols: 2,
        rows: 1,
        cells: vec![cell; 2],
        cursor: None,
        display_offset: 0,
        history_len: 0,
    };
    assert_eq!(snapshot.row(0).len(), 2);
    assert!(snapshot.row(1).is_empty());
}
