use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentProject {
    pub name: String,
    pub path: PathBuf,
}

impl RecentProject {
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into().trim().to_string(),
            path: path.into(),
        }
    }

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

pub fn sanitize_target(name: &str) -> Option<String> {
    let mut out = String::new();
    let mut pending_sep = false;

    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('_');
            }
            pending_sep = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_sep = true;
        }
    }

    let target: String = out
        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '_')
        .to_string();

    if target.is_empty() {
        None
    } else {
        Some(target)
    }
}

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

    #[test]
    fn sanitize_spaces_to_snake_case() {
        assert_eq!(sanitize_target("My Fuzz").as_deref(), Some("my_fuzz"));
    }

    #[test]
    fn sanitize_collapses_symbol_runs() {
        assert_eq!(sanitize_target("a  --  b!!c").as_deref(), Some("a_b_c"));
    }

    #[test]
    fn sanitize_strips_leading_digits_and_underscores() {
        assert_eq!(sanitize_target("__9lives").as_deref(), Some("lives"));
        assert_eq!(sanitize_target("123abc").as_deref(), Some("abc"));
    }

    #[test]
    fn sanitize_all_symbols_is_none() {
        assert_eq!(sanitize_target("###"), None);
        assert_eq!(sanitize_target("   "), None);
        assert_eq!(sanitize_target("007"), None);
    }

    #[test]
    fn sanitize_drops_non_ascii() {
        assert_eq!(
            sanitize_target("Café Reverb").as_deref(),
            Some("caf_reverb")
        );
    }
}
