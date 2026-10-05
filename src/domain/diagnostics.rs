use std::collections::HashSet;
use std::ops::Range;

use crate::domain::editing;

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

pub fn parse_diagnostics(output: &str) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut seen: HashSet<Diagnostic> = HashSet::new();

    for line in output.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }

        if let Some(note) = parse_note(line) {
            if let Some(last) = diagnostics.last_mut() {
                last.notes.push(note);
            }
            continue;
        }

        let Some(diagnostic) = parse_line(line) else {
            continue;
        };
        if seen.insert(diagnostic.clone()) {
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}

fn parse_line(line: &str) -> Option<Diagnostic> {
    parse_gcc_clang(line)
        .or_else(|| parse_msvc(line))
        .or_else(|| parse_tool_error(line))
}

fn parse_gcc_clang(line: &str) -> Option<Diagnostic> {
    let (location, rest) = split_at_severity(line)?;
    let (severity, message) = rest;

    let mut parts = location.rsplitn(3, ':');
    let last = parts.next()?;
    let middle = parts.next();
    let head = parts.next();

    let (file, line_no, column) = match (head, middle) {
        // file:line:col
        (Some(file), Some(line_no)) => (
            file,
            line_no.trim().parse::<u32>().ok()?,
            last.trim().parse::<u32>().ok(),
        ),
        // file:line
        (None, Some(file)) => (file, last.trim().parse::<u32>().ok()?, None),
        _ => return None,
    };

    if file.trim().is_empty() {
        return None;
    }

    Some(Diagnostic {
        severity,
        file: Some(file.trim().to_string()),
        line: Some(line_no),
        column,
        message: message.to_string(),
        notes: Vec::new(),
    })
}

fn split_at_severity(line: &str) -> Option<(&str, (Severity, &str))> {
    for (keyword, severity) in [
        (": error: ", Severity::Error),
        (": fatal error: ", Severity::Error),
        (": warning: ", Severity::Warning),
    ] {
        if let Some(at) = line.find(keyword) {
            let location = &line[..at];
            let message = line[at + keyword.len()..].trim();
            if !message.is_empty() {
                return Some((location, (severity, message)));
            }
        }
    }
    None
}

fn parse_msvc(line: &str) -> Option<Diagnostic> {
    let (head, rest) = line.split_once("): ")?;
    let (file, position) = head.split_once('(')?;
    if file.trim().is_empty() {
        return None;
    }

    let (line_no, column) = match position.split_once(',') {
        Some((line_no, column)) => (line_no, column.trim().parse::<u32>().ok()),
        None => (position, None),
    };
    let line_no = line_no.trim().parse::<u32>().ok()?;

    let severity = if rest.starts_with("error") {
        Severity::Error
    } else if rest.starts_with("warning") {
        Severity::Warning
    } else {
        return None;
    };

    Some(Diagnostic {
        severity,
        file: Some(file.trim().to_string()),
        line: Some(line_no),
        column,
        message: rest.trim().to_string(),
        notes: Vec::new(),
    })
}

fn parse_tool_error(line: &str) -> Option<Diagnostic> {
    let trimmed = line.trim();

    if trimmed.starts_with("make:") && trimmed.contains("***") {
        return Some(tool_error(trimmed));
    }

    if let Some(rest) = trimmed.strip_prefix("ld: ")
        && !rest.trim().is_empty()
    {
        return Some(tool_error(trimmed));
    }

    if let Some((tool, rest)) = trimmed.split_once(": error: ")
        && !tool.contains(' ')
        && !rest.trim().is_empty()
    {
        return Some(tool_error(trimmed));
    }

    None
}

fn tool_error(message: &str) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        file: None,
        line: None,
        column: None,
        message: message.to_string(),
        notes: Vec::new(),
    }
}

fn parse_note(line: &str) -> Option<String> {
    let at = line.find(": note: ")?;
    let note = line[at + ": note: ".len()..].trim();
    (!note.is_empty()).then(|| note.to_string())
}

pub fn byte_column_to_char(line: &str, column: u32) -> usize {
    let byte = (column.max(1) - 1) as usize;
    if byte >= line.len() {
        return line.chars().count();
    }
    let mut boundary = byte;
    while boundary > 0 && !line.is_char_boundary(boundary) {
        boundary -= 1;
    }
    line[..boundary].chars().count()
}

const FALLBACK_SPAN: usize = 3;

pub fn span_in(text: &str, line: u32, column: Option<u32>) -> Option<Range<usize>> {
    let line_index = (line.max(1) - 1) as usize;

    let mut line_start = 0usize;
    let mut current = 0usize;
    for (index, line_text) in text.split('\n').enumerate() {
        if index == line_index {
            line_start = current;
            let column_chars = column.map_or(0, |c| byte_column_to_char(line_text, c));
            return Some(span_within_line(
                line_text,
                line_start,
                column_chars.min(line_text.chars().count()),
            ));
        }
        current += line_text.chars().count() + 1; // +1 for the newline
    }

    let _ = line_start;
    None
}

fn span_within_line(line_text: &str, line_start: usize, column: usize) -> Range<usize> {
    let length = line_text.chars().count();

    if let Some(word) = editing::word_at(line_text, column) {
        return (line_start + word.start)..(line_start + word.end);
    }

    let start = column.min(length);
    let end = (start + FALLBACK_SPAN).min(length).max(start);
    (line_start + start)..(line_start + end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(output: &str) -> Diagnostic {
        let parsed = parse_diagnostics(output);
        assert_eq!(parsed.len(), 1, "expected exactly one, got {parsed:?}");
        parsed.into_iter().next().unwrap()
    }

    // -- gcc / clang --------------------------------------------------------

    #[test]
    fn a_clang_error_parses_into_its_parts() {
        let d = one("src/effect.cpp:14:24: error: use of undeclared identifier 'cutof'");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.file.as_deref(), Some("src/effect.cpp"));
        assert_eq!(d.line, Some(14));
        assert_eq!(d.column, Some(24));
        assert_eq!(d.message, "use of undeclared identifier 'cutof'");
        assert_eq!(d.location().as_deref(), Some("effect.cpp:14"));
        assert!(d.has_position());
    }

    #[test]
    fn a_gcc_warning_keeps_the_flag_that_produced_it() {
        let d =
            one("effect_processor.cpp:9:11: warning: unused variable 'dryMix' [-Wunused-variable]");
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.line, Some(9));
        assert_eq!(d.column, Some(11));
        assert!(
            d.message.ends_with("[-Wunused-variable]"),
            "the flag tells the user what to silence: {}",
            d.message
        );
    }

    #[test]
    fn a_fatal_error_counts_as_an_error() {
        let d = one("dsp.h:1:10: fatal error: 'missing.h' file not found");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.message, "'missing.h' file not found");
    }

    #[test]
    fn a_diagnostic_without_a_column_still_parses() {
        let d = one("Makefile:31: warning: overriding recipe for target 'dylib'");
        assert_eq!(d.line, Some(31));
        assert_eq!(d.column, None);
        assert!(d.has_position(), "a line alone is enough to jump to");
    }

    #[test]
    fn a_windows_drive_letter_does_not_confuse_the_split() {
        // Splitting left-to-right on ':' would take "C" as the file.
        let d = one(r"C:\src\effect.cpp:14:24: error: boom");
        assert_eq!(d.file.as_deref(), Some(r"C:\src\effect.cpp"));
        assert_eq!(d.line, Some(14));
        assert_eq!(d.column, Some(24));
        assert_eq!(d.location().as_deref(), Some("effect.cpp:14"));
    }

    // -- notes --------------------------------------------------------------

    #[test]
    fn notes_fold_into_the_diagnostic_above_them() {
        let output = "\
src/effect.cpp:14:24: error: use of undeclared identifier 'cutof'
src/effect.cpp:9:11: note: did you mean 'cutoff'?
src/effect.cpp:9:11: note: 'cutoff' declared here";
        let d = one(output);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(
            d.notes,
            vec!["did you mean 'cutoff'?", "'cutoff' declared here"],
            "one mistake must stay one row in the strip"
        );
    }

    #[test]
    fn a_note_with_nothing_before_it_is_dropped_rather_than_promoted() {
        assert!(parse_diagnostics("a.cpp:1:1: note: orphan").is_empty());
    }

    // -- msvc ---------------------------------------------------------------

    #[test]
    fn the_msvc_format_parses_too() {
        let d = one("effect.cpp(14,24): error C2065: 'cutof': undeclared identifier");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.file.as_deref(), Some("effect.cpp"));
        assert_eq!(d.line, Some(14));
        assert_eq!(d.column, Some(24));
        assert!(d.message.starts_with("error C2065"));
    }

    #[test]
    fn an_msvc_warning_without_a_column_parses() {
        let d = one("effect.cpp(9): warning C4101: 'dryMix': unreferenced local variable");
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.line, Some(9));
        assert_eq!(d.column, None);
    }

    // -- tool errors --------------------------------------------------------

    #[test]
    fn tool_failures_count_but_have_nowhere_to_jump() {
        for line in [
            "ld: symbol(s) not found for architecture arm64",
            "clang++: error: linker command failed with exit code 1 (use -v to see invocation)",
            "make: *** [build/libtest3.dylib] Error 1",
        ] {
            let d = one(line);
            assert_eq!(d.severity, Severity::Error, "{line}");
            assert!(!d.has_position(), "{line} has no source position");
            assert_eq!(d.location(), None, "{line}");
            assert_eq!(d.message, line);
        }
    }

    // -- non-diagnostics ----------------------------------------------------

    #[test]
    fn ordinary_build_chatter_is_not_mistaken_for_a_diagnostic() {
        let output = "\
clang++ -std=c++17 -O2 -fPIC -dynamiclib hl_adapter.cpp -o build/libtest3.dylib
mkdir -p build
Note: some builds print prose with: colons in it
https://example.com/docs:1:2
";
        assert!(
            parse_diagnostics(output).is_empty(),
            "got {:?}",
            parse_diagnostics(output)
        );
    }

    #[test]
    fn a_severity_word_without_a_message_is_not_a_diagnostic() {
        assert!(parse_diagnostics("a.cpp:1:1: error: ").is_empty());
        assert!(parse_diagnostics("a.cpp:1:1: error:").is_empty());
    }

    // -- ordering and duplicates --------------------------------------------

    #[test]
    fn diagnostics_keep_build_order() {
        let output = "\
b.cpp:2:1: warning: first
a.cpp:1:1: error: second
c.cpp:3:1: error: third";
        let parsed = parse_diagnostics(output);
        let messages: Vec<&str> = parsed.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages, vec!["first", "second", "third"]);
    }

    #[test]
    fn an_identical_diagnostic_from_several_translation_units_is_listed_once() {
        // A warning in a shared header repeats per .cpp that includes it.
        let output = "\
dsp_primitives.h:22:9: warning: unused parameter 'x' [-Wunused-parameter]
dsp_primitives.h:22:9: warning: unused parameter 'x' [-Wunused-parameter]
dsp_primitives.h:22:9: warning: unused parameter 'x' [-Wunused-parameter]";
        let parsed = parse_diagnostics(output);
        assert_eq!(parsed.len(), 1);
        assert_eq!(
            counts(&parsed),
            Counts {
                errors: 0,
                warnings: 1
            }
        );
    }

    #[test]
    fn the_same_line_with_a_different_message_is_kept() {
        let output = "\
a.cpp:1:1: error: first problem
a.cpp:1:1: error: second problem";
        assert_eq!(parse_diagnostics(output).len(), 2);
    }

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

    // -- columns ------------------------------------------------------------

    #[test]
    fn a_byte_column_becomes_a_character_offset() {
        // "  float cutof" — column 9 (1-based bytes) is the 'c' of cutof.
        let line = "  float cutof = 0.5f;";
        assert_eq!(byte_column_to_char(line, 9), 8);
        assert_eq!(&line[8..13], "cutof");
    }

    #[test]
    fn multi_byte_characters_shift_the_column() {
        // "// é" is 5 bytes but 4 characters; a byte column past the é must not
        // be used as a character offset directly.
        let line = "// é x";
        // Byte 6 (1-based) is the 'x'; as characters that is offset 5.
        assert_eq!(byte_column_to_char(line, 7), 5);
        assert_eq!(line.chars().nth(5), Some('x'));
    }

    // -- spans --------------------------------------------------------------

    const SOURCE: &str = "void f() {\n  float cutof = 0.5f;\n  return;\n}\n";

    #[test]
    fn a_span_covers_the_identifier_at_the_column() {
        // Line 2, byte column 9 is the 'c' of `cutof`.
        let span = span_in(SOURCE, 2, Some(9)).unwrap();
        let text: String = SOURCE.chars().take(span.end).skip(span.start).collect();
        assert_eq!(text, "cutof", "the squiggle must cover the whole token");
    }

    #[test]
    fn a_span_on_punctuation_falls_back_to_a_short_run() {
        // Line 2 column 15 is the '=' — not an identifier character.
        let span = span_in(SOURCE, 2, Some(15)).unwrap();
        assert_eq!(span.len(), FALLBACK_SPAN);
        let text: String = SOURCE.chars().take(span.end).skip(span.start).collect();
        assert_eq!(text, "= 0");
    }

    #[test]
    fn a_span_without_a_column_starts_at_the_line() {
        let span = span_in(SOURCE, 3, None).unwrap();
        // Column 0 of "  return;" is a space, so the fallback run applies.
        let text: String = SOURCE.chars().take(span.end).skip(span.start).collect();
        assert_eq!(text, "  r");
    }

    #[test]
    fn a_span_past_the_end_of_the_line_clamps_rather_than_panicking() {
        let span = span_in(SOURCE, 3, Some(500)).unwrap();
        assert!(span.start <= SOURCE.chars().count());
        assert!(span.end <= SOURCE.chars().count());
        assert!(span.start <= span.end);
    }

    #[test]
    fn a_line_the_buffer_does_not_have_yields_nothing() {
        // The file may have been edited, or shrunk, since the build.
        assert_eq!(span_in(SOURCE, 999, Some(1)), None);
        assert_eq!(span_in("", 5, Some(1)), None);
    }

    #[test]
    fn spans_are_character_offsets_even_with_multi_byte_text() {
        let source = "// héllo\n  float cutof = 0;\n";
        let span = span_in(source, 2, Some(9)).unwrap();
        let text: String = source.chars().take(span.end).skip(span.start).collect();
        assert_eq!(
            text, "cutof",
            "a multi-byte first line must not shift the second line's span"
        );
    }

    #[test]
    fn out_of_range_columns_clamp_instead_of_panicking() {
        let line = "short";
        assert_eq!(byte_column_to_char(line, 999), 5);
        // Compilers are 1-based; 0 would underflow.
        assert_eq!(byte_column_to_char(line, 0), 0);
        assert_eq!(byte_column_to_char("", 1), 0);
    }
}
