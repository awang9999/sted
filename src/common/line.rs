use crate::common::{
    constants::TAB_WIDTH_SPACES,
    textfragment::{GraphemeWidth, TextFragment},
};
use std::fmt;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone)]
pub struct Line {
    content: Vec<TextFragment>,
    pub scroll_x: usize,
}

/// Renders the line's raw text content (the actual graphemes), not the
/// on-screen representation. This is the round-trippable form used when
/// re-parsing a line's content; use [`Line::get`] for what is drawn.
impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for fragment in &self.content {
            f.write_str(&fragment.grapheme)?;
        }
        Ok(())
    }
}

impl Line {
    pub fn newline() -> Self {
        Self {
            content: Self::convert_string_to_content(""),
            scroll_x: 0,
        }
    }

    pub fn from(line_str: &str) -> Self {
        let content = Self::convert_string_to_content(line_str);

        Self {
            content: content,
            scroll_x: 0,
        }
    }

    /// Appends the content of `other` to this line, keeping this line's existing content first.
    pub fn append(&mut self, other: Self) {
        self.content.extend(other.content);
        // we need to convert the full line into a string and then back to vec<TextFragment>
        // in case appending other to self modifies the edge graphemes
        let content_string = self.to_string();
        self.content = Line::convert_string_to_content(&content_string);
    }

    fn convert_string_to_content(line_str: &str) -> Vec<TextFragment> {
        let fragments = line_str
            .graphemes(true)
            .map(|grapheme| {
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

                let replacement = Line::replacement_character(grapheme);
                TextFragment::from(grapheme, rendered_width, replacement)
            })
            .collect::<Vec<_>>();

        // Merge fragments that form a compound emoji via U+200D.
        let mut merged: Vec<TextFragment> = Vec::with_capacity(fragments.len());
        let mut i = 0;
        while i < fragments.len() {
            let mut current = fragments[i].clone();
            while i + 1 < fragments.len()
                && (fragments[i].grapheme.ends_with('\u{200d}')
                    || fragments[i + 1].grapheme.starts_with('\u{200d}'))
            {
                let next_str = &fragments[i + 1].grapheme;
                let combined_str = format!("{}{}", current.grapheme, next_str);

                let width = UnicodeWidthStr::width(combined_str.as_str());
                let rendered_width = if width > 1 {
                    GraphemeWidth::Full
                } else {
                    GraphemeWidth::Half
                };
                let replacement = Line::replacement_character(combined_str.as_str());

                current.grapheme = combined_str;
                current.rendered_width = rendered_width;
                current.replacement = replacement;

                i += 1; // skip merged fragment
            }
            merged.push(current);
            i += 1;
        }

        merged
    }

    // Given a view offset x and a terminal width, returns
    // the visible portion of the Line. If the start or end
    // of the visible range cuts off a grapheme, the character
    // U+2026 (…) replaces it.
    pub fn get(&self, range: Range<usize>) -> String {
        if self.content.is_empty() {
            return String::new();
        }

        let mut segment = String::new();
        let mut idx = 0; // track index of grapheme vector

        // Skip past graphemes before the visible range
        while idx < self.content.len() && self.width_until(idx) < range.start {
            idx += 1;
        }

        // Truncation at range.start?
        if idx > 0 && self.width_until(idx - 1) < range.start {
            segment.push_str("…");
            idx += 1;
        }

        // Add graphemes within the visible range
        while idx < self.content.len() && self.width_until(idx + 1) <= range.end {
            segment.push_str(&self.content[idx].get_display_text());
            idx += 1;
        }

        // Truncation at range.end?
        if idx < self.content.len() && self.width_until(idx) > range.end {
            segment.push_str("…");
        }

        segment
    }

    pub fn grapheme_count(&self) -> usize {
        return self.content.len();
    }

    pub fn width_until(&self, grapheme_index: usize) -> usize {
        self.content
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
            _ if for_str.chars().all(|c| c.is_control()) => Some('▯'.to_string()),
            _ => None,
        }
    }

    /// Inserts character `c` at `grapheme_index`, shifting all subsequent graphemes right.
    pub fn insert_char(&mut self, grapheme_index: usize, c: char) {
        let mut result = String::new();
        let mut idx = 0; // track index of grapheme vector

        // Build string using the TextFragment graphemes up to the insertion index
        while idx < grapheme_index && idx < self.content.len() {
            result.push_str(&self.content[idx].grapheme);
            idx += 1;
        }

        result.push_str(&format!("{c}"));

        // insert remaining TextFragments as graphemes
        while idx < self.content.len() {
            result.push_str(&self.content[idx].grapheme);
            idx += 1;
        }

        self.content = Self::convert_string_to_content(&result);
    }

    /// Splits this line at `grapheme_index`: the graphemes before the index
    /// stay in place and a new `Line` containing the remainder is returned.
    ///
    /// The index is clamped to the line's length, so splitting an empty line
    /// or splitting past the end of the line leaves this line intact and
    /// yields an empty remainder.
    pub fn split_at(&mut self, grapheme_index: usize) -> Line {
        let index = grapheme_index.min(self.content.len());
        let remainder = self.content.split_off(index);
        Self {
            content: remainder,
            scroll_x: 0,
        }
    }

    /// Removes the grapheme cluster at `grapheme_index` from this line's content.
    pub fn delete_grapheme(&mut self, grapheme_index: usize) {
        if grapheme_index >= self.content.len() {
            return;
        };
        self.content.remove(grapheme_index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_grapheme_removes_correct_grapheme() {
        let mut line = Line::from("hello");
        assert_eq!(line.grapheme_count(), 5);

        line.delete_grapheme(2); // removes 'l'
        assert_eq!(
            line.to_string(),
            "helo"
        );
    }

    #[test]
    fn delete_grapheme_at_start_and_end() {
        let mut line = Line::from("abc");

        line.delete_grapheme(0); // remove 'a'
        assert_eq!(
            line.to_string(),
            "bc"
        );

        let mut line = Line::from("abc");
        line.delete_grapheme(2); // remove 'c' — left with ["a", "b"]
        assert_eq!(
            line.to_string(),
            "ab"
        );

        // On the same line, delete at index 0 removes 'a', leaving ['b']
        line.delete_grapheme(0);
        assert_eq!(
            line.to_string(),
            "b"
        );
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
        assert_eq!(line.content.len(), 0);
    }

    #[test]
    fn delete_grapheme_removes_whole_emoji_grapheme() {
        // Family emoji uses a ZWJ sequence; convert_string_to_content merges
        // it into a single TextFragment.  Deleting at any index that would
        // land in the middle shouldn't happen because the whole fragment is
        // one grapheme, but deleting the fragment itself should work.
        let mut line = Line::from("👨‍👩‍👧");
        assert!(line.grapheme_count() >= 1);

        line.delete_grapheme(0);
        assert_eq!(line.content.len(), 0);
    }

    #[test]
    fn insert_char_at_beginning() {
        let mut line = Line::from("bc");
        line.insert_char(0, 'a');
        assert_eq!(
            line.to_string(),
            "abc"
        );
    }

    #[test]
    fn insert_char_in_middle() {
        let mut line = Line::from("ac");
        line.insert_char(1, 'b');
        assert_eq!(
            line.to_string(),
            "abc"
        );
    }

    #[test]
    fn insert_char_at_end() {
        let mut line = Line::from("ab");
        line.insert_char(2, 'c');
        assert_eq!(
            line.to_string(),
            "abc"
        );
    }

    #[test]
    fn insert_char_into_empty_line() {
        let mut line = Line::from("");
        line.insert_char(0, 'x');
        assert_eq!(
            line.to_string(),
            "x"
        );
        assert_eq!(line.grapheme_count(), 1);
    }

    #[test]
    fn insert_char_out_of_bounds_pushes_at_end() {
        let mut line = Line::from("abc");
        // Insert past the end — should still append at EOF
        line.insert_char(10, 'z');
        assert_eq!(line.grapheme_count(), 4);
        assert_eq!(
            line.to_string(),
            "abcz"
        );
    }

    #[test]
    fn insert_char_into_empty_line_out_of_bounds() {
        let mut line = Line::from("");
        // Empty line: index 5 is out of bounds, should still work
        line.insert_char(5, 'w');
        assert_eq!(line.grapheme_count(), 1);
        assert_eq!(
            line.to_string(),
            "w"
        );
    }

    #[test]
    fn insert_char_multiple_times_preserves_order() {
        let mut line = Line::from("");
        line.insert_char(0, 'c');
        line.insert_char(0, 'b');
        line.insert_char(0, 'a');
        assert_eq!(
            line.to_string(),
            "abc"
        );
    }

    #[test]
    fn insert_char_preserves_existing_graphemes() {
        let mut line = Line::from("hello");
        line.insert_char(3, 'X');
        assert_eq!(
            line.to_string(),
            "helXlo"
        );

        // Verify remaining graphemes are intact
        line.insert_char(6, 'Y');
        assert_eq!(
            line.to_string(),
            "helXloY"
        );
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
        assert_eq!(
            line.to_string(),
            "Hhello"
        );
    }

    #[test]
    fn split_at_mid_line_splits_into_prefix_and_remainder() {
        let mut line = Line::from("abcdef");
        let after = line.split_at(3);
        assert_eq!(
            line.to_string(),
            "abc"
        );
        assert_eq!(
            after.to_string(),
            "def"
        );
    }

    #[test]
    fn split_at_beginning_of_line_yields_empty_prefix() {
        let mut line = Line::from("abcdef");
        let after = line.split_at(0);
        assert_eq!(line.grapheme_count(), 0);
        assert_eq!(
            after.to_string(),
            "abcdef"
        );
    }

    #[test]
    fn split_at_end_of_line_yields_empty_remainder() {
        let mut line = Line::from("abcdef");
        let after = line.split_at(6);
        assert_eq!(
            line.to_string(),
            "abcdef"
        );
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
        assert_eq!(
            line.to_string(),
            "abc"
        );
        assert_eq!(after.grapheme_count(), 0);
    }

    #[test]
    fn split_at_never_splits_a_merged_emoji_fragment() {
        let mut line = Line::from("a👨‍👩‍👧b");
        // 'a', the family emoji (merged into one fragment), 'b'
        assert_eq!(line.grapheme_count(), 3);

        let after = line.split_at(2);

        assert_eq!(
            line.to_string(),
            "a👨‍👩‍👧"
        );
        assert_eq!(
            after.to_string(),
            "b"
        );
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
}
