use crate::common::{
    constants::TAB_WIDTH_SPACES,
    textfragment::{GraphemeWidth, TextFragment},
};
use std::fmt;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// A single line of a buffer.
///
/// The line's text is stored verbatim in `text` and is the single source
/// of truth: edit operations modify it in place at the byte positions the
/// fragments remember, and a search can work on it directly without
/// assembling it. `fragments` is layout information derived from `text` —
/// one entry per grapheme cluster, each carrying its byte index in `text`
/// (as reported by `grapheme_indices`) so grapheme indices can be
/// translated into byte positions without scanning the line.
#[derive(Clone)]
pub struct Line {
    /// The raw text of the line, kept verbatim.
    text: String,
    /// Layout information derived from `text`: one fragment per grapheme
    /// cluster.
    fragments: Vec<TextFragment>,
    pub scroll_x: usize,
}

/// Writes the line's raw text (the actual graphemes), not the on-screen
/// representation; use [`Line::get`] for what is drawn.
impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl Line {
    pub fn newline() -> Self {
        Self::from("")
    }

    pub fn from(line_str: &str) -> Self {
        Self {
            text: line_str.to_string(),
            fragments: Self::fragments_from(line_str),
            scroll_x: 0,
        }
    }

    /// The line's raw text, ready for byte-wise operations such as
    /// `str::find`.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The grapheme at `grapheme_index` as a slice of the line's text:
    /// from its byte index to the next fragment's byte index, or to the end
    /// of the text for the last grapheme.
    fn grapheme_text(&self, grapheme_index: usize) -> &str {
        let start = self.fragments[grapheme_index].byte_index;
        let end = Self::grapheme_end(&self.fragments, grapheme_index, self.text.len());
        &self.text[start..end]
    }

    /// The on-screen text of the grapheme at `grapheme_index`: the raw
    /// grapheme, or its replacement for invisible graphemes such as tabs
    /// and control characters.
    fn display_text_of(&self, grapheme_index: usize) -> &str {
        match &self.fragments[grapheme_index].replacement {
            Some(replacement) => replacement.as_str(),
            None => self.grapheme_text(grapheme_index),
        }
    }

    /// Appends the content of `other` to this line, keeping this line's existing content first.
    pub fn append(&mut self, other: &Self) {
        if other.text.is_empty() {
            return;
        }
        // Appending can join graphemes across the seam (a combining mark or
        // a ZWJ sequence), so re-segment from this line's last grapheme.
        let seam = self.fragments.len().saturating_sub(1);
        self.text.push_str(&other.text);
        self.resegment_from(seam);
    }

    /// Splits this line at `grapheme_index`: the graphemes before the index
    /// stay in place and a new `Line` containing the remainder is returned.
    ///
    /// The index is clamped to the line's length, so splitting an empty line
    /// or splitting past the end of the line leaves this line intact and
    /// yields an empty remainder.
    pub fn split_at(&mut self, grapheme_index: usize) -> Line {
        let index = grapheme_index.min(self.fragments.len());
        let byte_index = self.byte_index_at(index);

        // Cut the text at the grapheme boundary. A split never lands inside
        // a grapheme, so the kept fragments stay valid and only the text
        // changes; the remainder gets fresh fragments from its own text.
        let text = std::mem::take(&mut self.text);
        let (before, after) = text.split_at(byte_index);
        self.text = before.to_string();
        self.fragments.truncate(index);

        Line::from(after)
    }

    /// Inserts character `c` at `grapheme_index`, shifting all subsequent graphemes right.
    pub fn insert_char(&mut self, grapheme_index: usize, c: char) {
        let index = grapheme_index.min(self.fragments.len());
        let byte_index = self.byte_index_at(index);
        self.text.insert(byte_index, c);
        // The inserted character can join a neighbouring grapheme (a
        // combining mark or a ZWJ sequence), so re-segment from the grapheme
        // before the insertion point.
        self.resegment_from(index.saturating_sub(1));
    }

    /// Removes the grapheme cluster at `grapheme_index` from this line's content.
    pub fn delete_grapheme(&mut self, grapheme_index: usize) {
        let start = match self.fragments.get(grapheme_index) {
            Some(fragment) => fragment.byte_index,
            None => return,
        };
        let end = Self::grapheme_end(&self.fragments, grapheme_index, self.text.len());
        self.text.replace_range(start..end, "");
        // Deleting a grapheme can join its neighbours (a combining mark or a
        // ZWJ sequence), so re-segment from the grapheme before the deleted
        // one.
        self.resegment_from(grapheme_index.saturating_sub(1));
    }

    /// The byte index in the line's text where the grapheme at
    /// `grapheme_index` begins; indices past the end of the line yield the
    /// length of the text.
    fn byte_index_at(&self, grapheme_index: usize) -> usize {
        match self.fragments.get(grapheme_index) {
            Some(fragment) => fragment.byte_index,
            None => self.text.len(),
        }
    }

    /// Rebuilds the fragments from fragment `start` onward, reusing the
    /// untouched prefix. The line's text is the source of truth, so this
    /// re-derives the grapheme clusters and their byte indices for the part
    /// of the line an edit may have changed.
    fn resegment_from(&mut self, start: usize) {
        let start = start.min(self.fragments.len());
        // The region starts where fragment `start` began; the untouched
        // prefix keeps its fragments and their byte indices. When `start`
        // is at the (old) end of the fragments, the whole new text is the
        // region.
        let prefix_len = self
            .fragments
            .get(start)
            .map_or(0, |fragment| fragment.byte_index);
        let region = &self.text[prefix_len..];
        let mut suffix = Self::fragments_from(region);
        for fragment in &mut suffix {
            fragment.byte_index = fragment.byte_index.saturating_add(prefix_len);
        }
        self.fragments.truncate(start);
        self.fragments.extend(suffix);
    }

    /// Segments `line_str` into fragments: one per grapheme cluster, each
    /// remembering its byte index in `line_str`, its rendered width, and
    /// the replacement text used for invisible graphemes.
    fn fragments_from(line_str: &str) -> Vec<TextFragment> {
        let fragments = line_str
            .grapheme_indices(true)
            .map(|(byte_index, grapheme)| {
                let unicode_width = grapheme.width();
                // Tabs render as TAB_WIDTH_SPACES spaces, so they must be
                // measured with that width instead of their Unicode width of 0.
                let rendered_width = match grapheme {
                    "\t" => GraphemeWidth::Tab,
                    _ => match unicode_width {
                        0 | 1 => GraphemeWidth::Half,
                        _ => GraphemeWidth::Full,
                    },
                };

                let replacement = Self::replacement_character(grapheme);
                TextFragment::from(byte_index, rendered_width, replacement)
            })
            .collect::<Vec<_>>();

        Self::merge_zwj_fragments(line_str, &fragments)
    }

    /// Merges fragments that form a compound emoji via U+200D. The merged
    /// fragment keeps the first fragment's byte index and spans the merged
    /// graphemes' byte range in `line_str`.
    fn merge_zwj_fragments(line_str: &str, fragments: &[TextFragment]) -> Vec<TextFragment> {
        let mut merged: Vec<TextFragment> = Vec::with_capacity(fragments.len());
        let mut start = 0usize;
        while start < fragments.len() {
            let first = fragments[start].byte_index;
            let mut stop = start;
            let mut end = Self::grapheme_end(fragments, stop, line_str.len());
            // Extend while the merged range and the next raw grapheme are
            // joined via U+200D: the merged range ends in a ZWJ, or the
            // next grapheme begins with one.
            while stop.saturating_add(1) < fragments.len()
                && (line_str[first..end].ends_with('\u{200d}')
                    || line_str[end..].starts_with('\u{200d}'))
            {
                stop = stop.saturating_add(1);
                end = Self::grapheme_end(fragments, stop, line_str.len());
            }

            if stop > start {
                // Recompute width and replacement over the full merged range.
                let range = &line_str[first..end];
                let width = UnicodeWidthStr::width(range);
                let rendered_width = if width > 1 {
                    GraphemeWidth::Full
                } else {
                    GraphemeWidth::Half
                };
                let replacement = Self::replacement_character(range);
                merged.push(TextFragment::from(first, rendered_width, replacement));
            } else {
                merged.push(fragments[start].clone());
            }
            start = stop.saturating_add(1);
        }
        merged
    }

    /// The byte index just past the grapheme at `index`: the next
    /// fragment's start, or the end of the line holding `line_len` bytes.
    fn grapheme_end(fragments: &[TextFragment], index: usize, line_len: usize) -> usize {
        fragments
            .get(index.saturating_add(1))
            .map_or(line_len, |fragment| fragment.byte_index)
    }

    // Given a view offset x and a terminal width, returns
    // the visible portion of the Line. If the start or end
    // of the visible range cuts off a grapheme, the character
    // U+2026 (…) replaces it.
    pub fn get(&self, range: Range<usize>) -> String {
        if self.fragments.is_empty() {
            return String::new();
        }

        let mut segment = String::new();
        let mut idx = 0; // track index of grapheme vector

        // Skip past graphemes before the visible range
        while idx < self.fragments.len() && self.width_until(idx) < range.start {
            idx = idx.saturating_add(1);
        }

        // Truncation at range.start?
        if idx > 0 && self.width_until(idx.saturating_sub(1)) < range.start {
            segment.push('…');
            idx = idx.saturating_add(1);
        }

        // Add graphemes within the visible range
        while idx < self.fragments.len() && self.width_until(idx.saturating_add(1)) <= range.end {
            segment.push_str(self.display_text_of(idx));
            idx = idx.saturating_add(1);
        }

        // Truncation at range.end?
        if idx < self.fragments.len() && self.width_until(idx) > range.end {
            segment.push('…');
        }

        segment
    }

    pub fn grapheme_count(&self) -> usize {
        self.fragments.len()
    }

    /// The rendered form of the line's graphemes from `grapheme_index` to
    /// its end: tabs and control characters already replaced by their
    /// display text. Indices past the end of the line yield an empty string.
    pub fn display_text_from(&self, grapheme_index: usize) -> String {
        (grapheme_index.min(self.fragments.len())..self.fragments.len())
            .map(|index| self.display_text_of(index))
            .collect()
    }

    pub fn width_until(&self, grapheme_index: usize) -> usize {
        self.fragments
            .iter()
            .take(grapheme_index)
            .map(|fragment| match fragment.rendered_width {
                GraphemeWidth::Half => 1,
                GraphemeWidth::Full => 2,
                GraphemeWidth::Tab => TAB_WIDTH_SPACES,
            })
            .sum()
    }

    fn replacement_character(for_str: &str) -> Option<String> {
        let width = for_str.width();
        match for_str {
            // Spaces get rendered as spaces
            " " => None,
            // Tabs get rendered as spaces
            "\t" => Some(" ".repeat(TAB_WIDTH_SPACES)),
            // Any visible whitespace characters besides spaces and tabs
            _ if width > 0 && for_str.trim().is_empty() => Some('␣'.to_string()),
            // Control characters (one or more consecutive)
            _ if for_str.chars().all(char::is_control) => Some('▯'.to_string()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn byte_indices(line: &Line) -> Vec<usize> {
        line.fragments.iter().map(|f| f.byte_index).collect()
    }

    #[test]
    fn delete_grapheme_removes_correct_grapheme() {
        let mut line = Line::from("hello");
        assert_eq!(line.grapheme_count(), 5);

        line.delete_grapheme(2); // removes 'l'
        assert_eq!(line.to_string(), "helo");
    }

    #[test]
    fn delete_grapheme_at_start_and_end() {
        let mut line = Line::from("abc");

        line.delete_grapheme(0); // remove 'a'
        assert_eq!(line.to_string(), "bc");

        let mut line = Line::from("abc");
        line.delete_grapheme(2); // remove 'c' — left with ["a", "b"]
        assert_eq!(line.to_string(), "ab");

        // On the same line, delete at index 0 removes 'a', leaving ['b']
        line.delete_grapheme(0);
        assert_eq!(line.to_string(), "b");
    }

    #[test]
    fn delete_grapheme_out_of_bounds_is_noop() {
        let mut line = Line::from("abcd");
        let original_count = line.grapheme_count();

        // Index 4 is past the end (valid indices are 0..3)
        line.delete_grapheme(4);
        assert_eq!(line.grapheme_count(), original_count);

        line.delete_grapheme(100);
        assert_eq!(line.grapheme_count(), original_count);
    }

    #[test]
    fn delete_grapheme_empty_line_is_noop() {
        let mut line = Line::from("");
        line.delete_grapheme(0);
        assert_eq!(line.grapheme_count(), 0);
    }

    #[test]
    fn delete_grapheme_removes_whole_emoji_grapheme() {
        // Family emoji uses a ZWJ sequence; fragments_from merges
        // it into a single TextFragment.  Deleting at any index that would
        // land in the middle shouldn't happen because the whole fragment is
        // one grapheme, but deleting the fragment itself should work.
        let mut line = Line::from("👨‍👩‍👧");
        assert!(line.grapheme_count() >= 1);

        line.delete_grapheme(0);
        assert_eq!(line.grapheme_count(), 0);
    }

    #[test]
    fn insert_char_at_beginning() {
        let mut line = Line::from("bc");
        line.insert_char(0, 'a');
        assert_eq!(line.to_string(), "abc");
    }

    #[test]
    fn insert_char_in_middle() {
        let mut line = Line::from("ac");
        line.insert_char(1, 'b');
        assert_eq!(line.to_string(), "abc");
    }

    #[test]
    fn insert_char_at_end() {
        let mut line = Line::from("ab");
        line.insert_char(2, 'c');
        assert_eq!(line.to_string(), "abc");
    }

    #[test]
    fn insert_char_into_empty_line() {
        let mut line = Line::from("");
        line.insert_char(0, 'x');
        assert_eq!(line.to_string(), "x");
        assert_eq!(line.grapheme_count(), 1);
    }

    #[test]
    fn insert_char_out_of_bounds_pushes_at_end() {
        let mut line = Line::from("abc");
        // Insert past the end — should still append at EOF
        line.insert_char(10, 'z');
        assert_eq!(line.grapheme_count(), 4);
        assert_eq!(line.to_string(), "abcz");
    }

    #[test]
    fn insert_char_into_empty_line_out_of_bounds() {
        let mut line = Line::from("");
        // Empty line: index 5 is out of bounds, should still work
        line.insert_char(5, 'w');
        assert_eq!(line.grapheme_count(), 1);
        assert_eq!(line.to_string(), "w");
    }

    #[test]
    fn insert_char_multiple_times_preserves_order() {
        let mut line = Line::from("");
        line.insert_char(0, 'c');
        line.insert_char(0, 'b');
        line.insert_char(0, 'a');
        assert_eq!(line.to_string(), "abc");
    }

    #[test]
    fn insert_char_preserves_existing_graphemes() {
        let mut line = Line::from("hello");
        line.insert_char(3, 'X');
        assert_eq!(line.to_string(), "helXlo");

        // Verify remaining graphemes are intact
        line.insert_char(6, 'Y');
        assert_eq!(line.to_string(), "helXloY");
    }

    #[test]
    fn insert_char_with_emoji() {
        let mut line = Line::from("ab");
        line.insert_char(1, '👍');
        let text = line.to_string();
        assert!(text.contains('👍'));
        assert_eq!(line.grapheme_count(), 3);
    }

    #[test]
    fn insert_char_at_beginning_of_emoji_line() {
        let mut line = Line::from("👍bc");
        line.insert_char(0, 'a');
        let text = line.to_string();
        assert!(text.starts_with('a'));
        assert_eq!(line.grapheme_count(), 4);
    }

    #[test]
    fn insert_char_between_two_emoji() {
        let mut line = Line::from("👍🎉");
        line.insert_char(1, 'x');
        let text = line.to_string();
        assert_eq!(text, "👍x🎉");
        assert_eq!(line.grapheme_count(), 3);
    }

    #[test]
    fn insert_char_repeatedly_at_same_position() {
        let mut line = Line::from("abc");
        for i in 0..5 {
            line.insert_char(1, char::from_digit(i, 10).unwrap());
        }
        let text = line.to_string();
        // Each insertion at index 1 pushes the previous inserted characters right
        assert_eq!(text, "a43210bc");
    }

    #[test]
    fn insert_char_into_line_with_empty_graphemes() {
        // Insert between two normal chars on any existing grapheme count
        let mut line = Line::from("hello");
        line.insert_char(0, 'H');
        assert_eq!(line.to_string(), "Hhello");
    }

    #[test]
    fn split_at_mid_line_splits_into_prefix_and_remainder() {
        let mut line = Line::from("abcdef");
        let after = line.split_at(3);
        assert_eq!(line.to_string(), "abc");
        assert_eq!(after.to_string(), "def");
    }

    #[test]
    fn split_at_beginning_of_line_yields_empty_prefix() {
        let mut line = Line::from("abcdef");
        let after = line.split_at(0);
        assert_eq!(line.grapheme_count(), 0);
        assert_eq!(after.to_string(), "abcdef");
    }

    #[test]
    fn split_at_end_of_line_yields_empty_remainder() {
        let mut line = Line::from("abcdef");
        let after = line.split_at(6);
        assert_eq!(line.to_string(), "abcdef");
        assert_eq!(after.grapheme_count(), 0);
    }

    #[test]
    fn split_at_on_empty_line_yields_two_empty_lines() {
        let mut line = Line::from("");
        let after = line.split_at(0);
        assert_eq!(line.grapheme_count(), 0);
        assert_eq!(after.grapheme_count(), 0);
    }

    #[test]
    fn split_at_out_of_bounds_clamps_to_end_of_line() {
        let mut line = Line::from("abc");
        let after = line.split_at(100);
        assert_eq!(line.to_string(), "abc");
        assert_eq!(after.grapheme_count(), 0);
    }

    #[test]
    fn split_at_never_splits_a_merged_emoji_fragment() {
        let mut line = Line::from("a👨‍👩‍👧b");
        // 'a', the family emoji (merged into one fragment), 'b'
        assert_eq!(line.grapheme_count(), 3);

        let after = line.split_at(2);

        assert_eq!(line.to_string(), "a👨‍👩‍👧");
        assert_eq!(after.to_string(), "b");
    }

    #[test]
    fn width_until_counts_a_tab_as_tab_width_spaces() {
        let line = Line::from("ab\tx");
        assert_eq!(line.width_until(0), 0);
        assert_eq!(line.width_until(1), 1);
        assert_eq!(line.width_until(2), 2);
        // the tab occupies TAB_WIDTH_SPACES columns
        assert_eq!(line.width_until(3), 2 + TAB_WIDTH_SPACES);
        assert_eq!(line.width_until(4), 3 + TAB_WIDTH_SPACES);
    }

    #[test]
    fn width_until_on_tab_only_line_is_tab_width() {
        let line = Line::from("\t");
        assert_eq!(line.width_until(1), TAB_WIDTH_SPACES);
    }

    #[test]
    fn get_renders_a_tab_as_tab_width_spaces() {
        let line = Line::from("a\tb");
        assert_eq!(line.get(0..10), "a    b");
    }

    #[test]
    fn display_text_from_yields_the_whole_rendered_line_from_zero() {
        let line = Line::from("a\tb");
        assert_eq!(line.display_text_from(0), "a    b");
    }

    #[test]
    fn display_text_from_skips_the_graphemes_before_the_index() {
        let line = Line::from("a\tb");
        assert_eq!(line.display_text_from(1), "    b");
        assert_eq!(line.display_text_from(2), "b");
    }

    #[test]
    fn display_text_from_past_the_end_is_empty() {
        let line = Line::from("ab");
        assert_eq!(line.display_text_from(2), "");
        assert_eq!(line.display_text_from(100), "");
    }

    #[test]
    fn text_returns_the_verbatim_line_content() {
        let line = Line::from("café");
        assert_eq!(line.text(), "café");
        // Display writes the same verbatim text.
        assert_eq!(line.to_string(), line.text());
    }

    #[test]
    fn fragments_store_the_byte_index_of_their_grapheme() {
        // 'é' (U+00E9) occupies two bytes in UTF-8, so the fragments after
        // it must account for that.
        let line = Line::from("caféx");
        assert_eq!(byte_indices(&line), vec![0, 1, 2, 3, 5]);
    }

    #[test]
    fn insert_char_shifts_the_byte_indices_of_the_following_fragments() {
        let mut line = Line::from("ab");
        line.insert_char(1, 'X');
        assert_eq!(line.text(), "aXb");
        assert_eq!(byte_indices(&line), vec![0, 1, 2]);
    }

    #[test]
    fn insert_multibyte_char_shifts_byte_indices_by_its_utf8_length() {
        let mut line = Line::from("ab");
        line.insert_char(1, 'é');
        assert_eq!(line.text(), "aéb");
        // 'a' (1 byte) + 'é' (2 bytes) → 'b' starts at byte 3.
        assert_eq!(line.fragments[2].byte_index, 3);
    }

    #[test]
    fn insert_char_at_end_extends_the_text() {
        let mut line = Line::from("café");
        line.insert_char(4, 'x');
        assert_eq!(line.text(), "caféx");
        assert_eq!(line.fragments[4].byte_index, 5);
    }

    #[test]
    fn delete_grapheme_rebases_the_byte_indices_after_the_gap() {
        let mut line = Line::from("café");
        line.delete_grapheme(0); // remove 'c'
        assert_eq!(line.text(), "afé");
        assert_eq!(byte_indices(&line), vec![0, 1, 2]);
    }

    #[test]
    fn split_at_keeps_byte_indices_consistent_in_both_halves() {
        let mut line = Line::from("café");
        let after = line.split_at(2);
        assert_eq!(line.text(), "ca");
        assert_eq!(after.text(), "fé");
        assert_eq!(line.fragments[1].byte_index, 1);
        assert_eq!(after.fragments[1].byte_index, 1); // 'é' inside "fé"
    }

    #[test]
    fn appending_a_line_keeps_byte_indices_consistent() {
        let mut line = Line::from("ca");
        line.append(&Line::from("fé"));
        assert_eq!(line.text(), "café");
        assert_eq!(byte_indices(&line), vec![0, 1, 2, 3]);
    }

    #[test]
    fn appending_to_an_empty_line_yields_the_other_line() {
        let mut line = Line::from("");
        line.append(&Line::from("fé"));
        assert_eq!(line.text(), "fé");
        assert_eq!(line.grapheme_count(), 2);
        assert_eq!(byte_indices(&line), vec![0, 1]);
    }

    #[test]
    fn inserting_a_combining_mark_joins_the_previous_grapheme() {
        let mut line = Line::from("cafe");
        line.insert_char(3, '\u{301}'); // acute accent after 'f'
        assert_eq!(line.text(), "caf\u{301}e");
        // 'f' and the combining mark form one grapheme at byte 2.
        assert_eq!(line.grapheme_count(), 4);
        assert_eq!(line.grapheme_text(2), "f\u{301}");
        assert_eq!(line.fragments[2].byte_index, 2);
        assert_eq!(line.fragments[3].byte_index, 5);
    }

    #[test]
    fn deleting_a_combined_grapheme_rebases_the_following_fragments() {
        let mut line = Line::from("caf\u{301}e");
        // 'f' + combining mark is a single grapheme at index 2.
        line.delete_grapheme(2);
        assert_eq!(line.text(), "cae");
        assert_eq!(line.grapheme_count(), 3);
        assert_eq!(byte_indices(&line), vec![0, 1, 2]);
    }

    #[test]
    fn inserting_a_zwj_joins_the_neighbouring_emoji() {
        let mut line = Line::from("👍🎉");
        line.insert_char(1, '\u{200d}');
        assert_eq!(line.text(), "👍\u{200d}🎉");
        // The ZWJ joins the two emoji into one merged fragment.
        assert_eq!(line.grapheme_count(), 1);
        assert_eq!(line.fragments[0].byte_index, 0);
    }
}
