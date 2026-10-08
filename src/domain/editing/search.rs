//! Finding words and their occurrences: Select Next Occurrence and the find
//! bar's matches.

use std::ops::Range;

pub fn word_at(text: &str, index: usize) -> Option<Range<usize>> {
    let chars: Vec<char> = text.chars().collect();
    let index = index.min(chars.len());

    let anchor = if index < chars.len() && is_word(chars[index]) {
        index
    } else if index > 0 && is_word(chars[index - 1]) {
        index - 1
    } else {
        return None;
    };

    let mut start = anchor;
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let mut end = anchor + 1;
    while end < chars.len() && is_word(chars[end]) {
        end += 1;
    }
    Some(start..end)
}

pub fn next_occurrence(text: &str, needle: &str, from: usize) -> Option<Range<usize>> {
    let chars: Vec<char> = text.chars().collect();
    let needle: Vec<char> = needle.chars().collect();
    if needle.is_empty() || needle.len() > chars.len() {
        return None;
    }

    let last = chars.len() - needle.len();
    let begin = if from > last { 0 } else { from };
    let positions = last + 1;

    (0..positions)
        .map(|step| (begin + step) % positions)
        .find(|&at| chars[at..at + needle.len()] == needle[..] && bounded(&chars, at, &needle))
        .map(|at| at..at + needle.len())
}

pub fn find_matches(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let haystack = text.to_lowercase();
    let needle = query.to_lowercase();

    let mut char_of_byte = vec![0usize; haystack.len() + 1];
    for (chars, (byte, _)) in haystack.char_indices().enumerate() {
        char_of_byte[byte] = chars;
    }
    char_of_byte[haystack.len()] = haystack.chars().count();

    let query_chars = needle.chars().count();
    let mut matches = Vec::new();
    let mut from = 0;
    while let Some(found) = haystack[from..].find(&needle) {
        let byte = from + found;
        let start = char_of_byte[byte];
        matches.push(start..start + query_chars);
        from = byte + needle.len().max(1);
    }
    matches
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn bounded(chars: &[char], at: usize, needle: &[char]) -> bool {
    let start_ok = !is_word(needle[0]) || at == 0 || !is_word(chars[at - 1]);
    let end = at + needle.len();
    let end_ok = !is_word(needle[needle.len() - 1]) || end >= chars.len() || !is_word(chars[end]);
    start_ok && end_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_at_finds_identifiers_including_underscores_and_digits() {
        let text = "float set_gain2 = 0;";
        assert_eq!(word_at(text, 8), Some(6..15));
        assert_eq!(word_at(text, 6), Some(6..15));
        assert_eq!(word_at(text, 15), Some(6..15));
    }

    #[test]
    fn word_at_returns_nothing_between_words() {
        let text = "a  b";
        assert_eq!(word_at(text, 2), None);
        assert_eq!(word_at(text, 99), Some(3..4));
    }

    #[test]
    fn word_at_counts_characters_not_bytes() {
        let text = "// héllo world";
        assert_eq!(word_at(text, 4), Some(3..8));
    }

    #[test]
    fn next_occurrence_is_case_sensitive_and_word_bounded() {
        let text = "gain Gain pregain gain_smoothed gain";
        assert_eq!(next_occurrence(text, "gain", 1), Some(32..36));
    }

    #[test]
    fn next_occurrence_wraps_to_the_top() {
        let text = "gain = gain;";
        assert_eq!(next_occurrence(text, "gain", 8), Some(0..4));
    }

    #[test]
    fn next_occurrence_finds_a_hit_at_the_offset_it_starts_from() {
        let text = "a gain b";
        assert_eq!(next_occurrence(text, "gain", 2), Some(2..6));
    }

    #[test]
    fn next_occurrence_skips_word_bounds_for_punctuation_needles() {
        let text = "a->b->c";
        assert_eq!(next_occurrence(text, "->", 2), Some(4..6));
    }

    #[test]
    fn next_occurrence_handles_absent_and_oversized_needles() {
        assert_eq!(next_occurrence("short", "much longer", 0), None);
        assert_eq!(next_occurrence("abc", "", 0), None);
        assert_eq!(next_occurrence("abc", "zzz", 0), None);
    }

    #[test]
    fn finds_every_occurrence_case_insensitively() {
        let text = "float Gain; float gain;";
        assert_eq!(find_matches(text, "gain"), vec![6..10, 18..22]);
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(find_matches("anything", "").is_empty());
    }

    #[test]
    fn ranges_are_character_offsets_not_byte_offsets() {
        let text = "é gain";
        assert_eq!(find_matches(text, "gain"), vec![2..6]);
    }

    #[test]
    fn overlapping_scan_does_not_loop_forever() {
        assert_eq!(find_matches("aaaa", "aa"), vec![0..2, 2..4]);
    }
}
