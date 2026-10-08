//! What a build runs on each platform, and how its status reads.

use std::path::Path;

use super::shell::find_git_bash;
use super::{Platform, ShellChoice};

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
