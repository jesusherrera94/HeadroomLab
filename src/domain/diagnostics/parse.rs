//! Recognising compiler and tool diagnostics in raw build output: gcc/clang,
//! MSVC, and bare tool failures such as `make` and `ld`.

use std::collections::HashSet;

use super::{Diagnostic, Severity};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diagnostics::{Counts, counts};

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
}
