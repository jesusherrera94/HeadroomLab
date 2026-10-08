//! Choosing the shell a new terminal session runs, including finding Git Bash
//! on Windows.

use std::path::{Path, PathBuf};

use super::{Platform, ShellChoice};

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
