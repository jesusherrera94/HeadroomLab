pub const INDENT: &str = "  ";

pub const MAX_OPEN_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_HIGHLIGHT_BYTES: usize = 512 * 1024;
pub const MAX_HIGHLIGHT_LINES: usize = 5_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentContent {
    Text {
        text: String,
        highlight: bool,
        crlf: bool,
    },
    Binary,
    TooLarge {
        bytes: u64,
    },
}

impl DocumentContent {
    pub fn text(&self) -> Option<&str> {
        match self {
            DocumentContent::Text { text, .. } => Some(text),
            _ => None,
        }
    }

    pub fn is_editable(&self) -> bool {
        matches!(self, DocumentContent::Text { .. })
    }
}

pub fn classify(bytes: Vec<u8>) -> DocumentContent {
    let len = bytes.len() as u64;
    if len > MAX_OPEN_BYTES {
        return DocumentContent::TooLarge { bytes: len };
    }
    let Ok(raw) = String::from_utf8(bytes) else {
        return DocumentContent::Binary;
    };

    let crlf = raw.contains("\r\n");
    let text = if crlf { raw.replace("\r\n", "\n") } else { raw };

    let highlight = text.len() <= MAX_HIGHLIGHT_BYTES
        && text.lines().take(MAX_HIGHLIGHT_LINES + 1).count() <= MAX_HIGHLIGHT_LINES;

    DocumentContent::Text {
        text,
        highlight,
        crlf,
    }
}

pub fn to_disk_bytes(text: &str, crlf: bool) -> Vec<u8> {
    if crlf {
        text.replace('\n', "\r\n").into_bytes()
    } else {
        text.as_bytes().to_vec()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Cpp,
    C,
    Header,
    Make,
    Markdown,
    Json,
    PlainText,
}

pub fn language_for(file_name: &str) -> Language {
    match file_name {
        "Makefile" | "makefile" | "GNUmakefile" => return Language::Make,
        _ => {}
    }

    let ext = file_name
        .rsplit('.')
        .next()
        .filter(|e| *e != file_name) // no dot → no extension
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "cpp" | "cc" | "cxx" | "c++" => Language::Cpp,
        "c" => Language::C,
        "h" | "hpp" | "hh" | "hxx" => Language::Header,
        "mk" | "cmake" => Language::Make,
        "md" | "markdown" => Language::Markdown,
        "json" => Language::Json,
        _ => Language::PlainText,
    }
}

pub fn language_label(lang: Language) -> &'static str {
    match lang {
        Language::Cpp => "C++",
        Language::C => "C",
        Language::Header => "C++ Header",
        Language::Make => "Makefile",
        Language::Markdown => "Markdown",
        Language::Json => "JSON",
        Language::PlainText => "Plain Text",
    }
}

pub fn line_comment(lang: Language) -> Option<&'static str> {
    match lang {
        Language::Cpp | Language::C | Language::Header => Some("//"),
        Language::Make => Some("#"),
        Language::Markdown | Language::Json | Language::PlainText => None,
    }
}

pub fn syntect_token(lang: Language) -> &'static str {
    match lang {
        Language::Cpp | Language::Header => "cpp",
        Language::C => "c",
        Language::Make => "make",
        Language::Markdown => "md",
        Language::Json => "json",
        Language::PlainText => "txt",
    }
}

pub fn line_col_at(text: &str, char_index: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for c in text.chars().take(char_index) {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub fn leading_indent(line: &str) -> String {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

pub fn auto_indent_for(previous_line: &str) -> String {
    let mut indent = leading_indent(previous_line);
    if previous_line.trim_end().ends_with('{') {
        indent.push_str(INDENT);
    }
    indent
}

pub fn closing_pair(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        _ => None,
    }
}

pub fn dedent(line: &str) -> (String, usize) {
    if let Some(rest) = line.strip_prefix(INDENT) {
        return (rest.to_string(), INDENT.len());
    }
    if let Some(rest) = line.strip_prefix('\t') {
        return (rest.to_string(), 1);
    }
    let spaces = line.chars().take_while(|c| *c == ' ').count();
    if spaces > 0 {
        return (line[spaces..].to_string(), spaces);
    }
    (line.to_string(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_is_editable_text() {
        let doc = classify(Vec::new());
        assert_eq!(
            doc,
            DocumentContent::Text {
                text: String::new(),
                highlight: true,
                crlf: false,
            }
        );
        assert!(doc.is_editable());
    }

    #[test]
    fn ordinary_source_is_highlighted_text() {
        let doc = classify(b"int main() { return 0; }\n".to_vec());
        match doc {
            DocumentContent::Text {
                text,
                highlight,
                crlf,
            } => {
                assert!(highlight);
                assert!(!crlf);
                assert!(text.starts_with("int main"));
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn crlf_is_normalised_on_load_and_restored_on_save() {
        let doc = classify(b"a\r\nb\r\n".to_vec());
        let DocumentContent::Text { text, crlf, .. } = doc else {
            panic!("expected text");
        };
        assert!(crlf);
        assert_eq!(text, "a\nb\n");
        assert_eq!(to_disk_bytes(&text, crlf), b"a\r\nb\r\n".to_vec());
        assert_eq!(to_disk_bytes(&text, false), b"a\nb\n".to_vec());
    }

    #[test]
    fn invalid_utf8_is_binary() {
        // 0xFF is never valid UTF-8.
        assert_eq!(classify(vec![0x00, 0xFF, 0xFE]), DocumentContent::Binary);
    }

    #[test]
    fn oversized_file_is_too_large() {
        let bytes = vec![b'a'; (MAX_OPEN_BYTES + 1) as usize];
        assert_eq!(
            classify(bytes),
            DocumentContent::TooLarge {
                bytes: MAX_OPEN_BYTES + 1
            }
        );
    }

    #[test]
    fn file_at_the_open_cap_still_opens() {
        let bytes = vec![b'a'; MAX_OPEN_BYTES as usize];
        assert!(classify(bytes).is_editable());
    }

    #[test]
    fn large_text_stays_editable_but_loses_highlighting() {
        let bytes = vec![b'a'; MAX_HIGHLIGHT_BYTES + 1];
        match classify(bytes) {
            DocumentContent::Text { highlight, .. } => assert!(!highlight),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn many_lines_lose_highlighting_too() {
        let bytes = "x\n".repeat(MAX_HIGHLIGHT_LINES + 1).into_bytes();
        match classify(bytes) {
            DocumentContent::Text { highlight, .. } => assert!(!highlight),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn language_resolution_covers_specials_and_fallback() {
        assert_eq!(language_for("Makefile"), Language::Make);
        assert_eq!(language_for("effect_processor.cpp"), Language::Cpp);
        assert_eq!(language_for("dsp_primitives.h"), Language::Header);
        assert_eq!(language_for("main.c"), Language::C);
        assert_eq!(language_for("README.md"), Language::Markdown);
        assert_eq!(language_for("config.json"), Language::Json);
        assert_eq!(language_for("mystery.xyz"), Language::PlainText);
        assert_eq!(language_for("LICENSE"), Language::PlainText);
        assert_eq!(language_label(Language::Cpp), "C++");
    }

    #[test]
    fn auto_indent_follows_previous_line_and_opens_blocks() {
        assert_eq!(auto_indent_for("int x = 1;"), "");
        assert_eq!(auto_indent_for("    int x = 1;"), "    ");
        assert_eq!(auto_indent_for("void f() {"), INDENT);
        assert_eq!(auto_indent_for("  void f() {  "), format!("  {INDENT}"));
        assert_eq!(leading_indent("\t\tx"), "\t\t");
    }

    #[test]
    fn line_col_is_one_based_and_counts_characters() {
        let text = "int a;\nint b;\n";
        assert_eq!(line_col_at(text, 0), (1, 1));
        assert_eq!(line_col_at(text, 6), (1, 7));
        assert_eq!(line_col_at(text, 7), (2, 1));
        assert_eq!(line_col_at(text, 13), (2, 7));
        assert_eq!(line_col_at(text, 14), (3, 1));
        assert_eq!(line_col_at(text, 999), (3, 1));
        assert_eq!(line_col_at("héllo", 3), (1, 4));
    }

    #[test]
    fn pairs_close_brackets_and_quotes() {
        assert_eq!(closing_pair('('), Some(')'));
        assert_eq!(closing_pair('{'), Some('}'));
        assert_eq!(closing_pair('"'), Some('"'));
        assert_eq!(closing_pair('a'), None);
    }

    #[test]
    fn dedent_removes_one_level_or_what_exists() {
        assert_eq!(dedent("    x"), ("  x".to_string(), 2));
        assert_eq!(dedent("\tx"), ("x".to_string(), 1));
        assert_eq!(dedent(" x"), ("x".to_string(), 1));
        assert_eq!(dedent("x"), ("x".to_string(), 0));
    }
}
