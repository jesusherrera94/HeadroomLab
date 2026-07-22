//! A project the user has opened. Pure value type; serde derives are IO-free.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One entry in the "recent projects" list: a display name plus the absolute
/// directory it lives in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentProject {
    /// Display name; trimmed, non-empty.
    pub name: String,
    /// Absolute project directory.
    pub path: PathBuf,
}

impl RecentProject {
    /// Builds a project, trimming surrounding whitespace off the name.
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into().trim().to_string(),
            path: path.into(),
        }
    }

    /// Derives the 2-char badge shown in the recents list. Takes the first
    /// letter of the first two "words" (split on space, `-`, `_` or camelCase
    /// boundaries); falls back to the first two characters. Always uppercased.
    pub fn abbr(&self) -> String {
        let words = split_words(&self.name);
        let letters: Vec<char> = words
            .iter()
            .filter_map(|w| w.chars().next())
            .take(2)
            .collect();

        let abbr: String = if letters.len() >= 2 {
            letters.into_iter().collect()
        } else {
            self.name.chars().take(2).collect()
        };

        abbr.to_uppercase()
    }
}

/// Splits a name into significant words on space/`-`/`_` separators and
/// camelCase boundaries (lower→upper transitions).
fn split_words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev: Option<char> = None;

    for ch in name.chars() {
        if ch == ' ' || ch == '-' || ch == '_' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else {
            if let Some(p) = prev
                && p.is_lowercase()
                && ch.is_uppercase()
                && !current.is_empty()
            {
                words.push(std::mem::take(&mut current));
            }
            current.push(ch);
        }
        prev = Some(ch);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_name() {
        let p = RecentProject::new("  My Project  ", "/tmp/x");
        assert_eq!(p.name, "My Project");
    }

    #[test]
    fn abbr_from_space_boundary() {
        assert_eq!(RecentProject::new("Pedal Effects", "/x").abbr(), "PE");
    }

    #[test]
    fn abbr_from_camel_case() {
        assert_eq!(RecentProject::new("pedalEffects", "/x").abbr(), "PE");
    }

    #[test]
    fn abbr_from_separators() {
        assert_eq!(RecentProject::new("my-cool_thing", "/x").abbr(), "MC");
    }

    #[test]
    fn abbr_single_word_takes_first_two_chars() {
        assert_eq!(RecentProject::new("reverb", "/x").abbr(), "RE");
    }
}
