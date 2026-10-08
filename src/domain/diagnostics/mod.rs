mod parse;
mod span;

pub use parse::parse_diagnostics;
pub use span::{byte_column_to_char, span_in};

pub const MAX_LISTED: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    pub severity: Severity,

    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn location(&self) -> Option<String> {
        let file = self.file.as_ref()?;
        let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
        Some(match self.line {
            Some(line) => format!("{name}:{line}"),
            None => name.to_string(),
        })
    }
    pub fn has_position(&self) -> bool {
        self.file.is_some() && self.line.is_some()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub errors: usize,
    pub warnings: usize,
}

impl Counts {
    pub fn total(self) -> usize {
        self.errors + self.warnings
    }

    pub fn is_empty(self) -> bool {
        self.total() == 0
    }
}

pub fn counts(diagnostics: &[Diagnostic]) -> Counts {
    let mut counts = Counts::default();
    for diagnostic in diagnostics {
        match diagnostic.severity {
            Severity::Error => counts.errors += 1,
            Severity::Warning => counts.warnings += 1,
        }
    }
    counts
}

pub fn summary(counts: Counts) -> Option<String> {
    if counts.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if counts.errors > 0 {
        parts.push(plural(counts.errors, "error"));
    }
    if counts.warnings > 0 {
        parts.push(plural(counts.warnings, "warning"));
    }
    Some(parts.join(" · "))
}

fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- counts and summary -------------------------------------------------

    #[test]
    fn counts_and_summary_read_naturally() {
        let output = "\
a.cpp:1:1: error: one
a.cpp:2:1: error: two
b.cpp:3:1: warning: three";
        let parsed = parse_diagnostics(output);
        let counts = counts(&parsed);
        assert_eq!(
            counts,
            Counts {
                errors: 2,
                warnings: 1
            }
        );
        assert_eq!(summary(counts).as_deref(), Some("2 errors · 1 warning"));
    }

    #[test]
    fn one_of_each_is_singular() {
        let counts = Counts {
            errors: 1,
            warnings: 1,
        };
        assert_eq!(summary(counts).as_deref(), Some("1 error · 1 warning"));
    }

    #[test]
    fn only_the_severity_that_occurred_is_mentioned() {
        assert_eq!(
            summary(Counts {
                errors: 0,
                warnings: 3
            })
            .as_deref(),
            Some("3 warnings")
        );
        assert_eq!(
            summary(Counts {
                errors: 4,
                warnings: 0
            })
            .as_deref(),
            Some("4 errors")
        );
    }

    #[test]
    fn a_clean_build_says_nothing_at_all() {
        // The segment must be absent, not "0 errors".
        assert_eq!(summary(Counts::default()), None);
        assert!(Counts::default().is_empty());
    }

    #[test]
    fn a_location_without_a_line_is_just_the_file_name() {
        let d = Diagnostic {
            severity: Severity::Warning,
            file: Some("src/dsp/filter.h".into()),
            line: None,
            column: None,
            message: "m".into(),
            notes: Vec::new(),
        };
        assert_eq!(d.location().as_deref(), Some("filter.h"));
        assert!(!d.has_position(), "a file alone is not enough to jump to");
    }

    #[test]
    fn severities_read_as_the_compiler_spells_them() {
        assert_eq!(Severity::Error.label(), "error");
        assert_eq!(Severity::Warning.label(), "warning");
    }
}
