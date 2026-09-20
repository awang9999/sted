use crate::common::{
    constants::TAB_WIDTH_SPACES,
    textfragment::{GraphemeWidth, TextFragment},
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone)]
pub struct Line {
    content: Vec<TextFragment>,
    pub scroll_x: usize,
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

    pub fn convert_content_to_string(&self, range: Range<usize>) -> String {
        self.content[range]
            .iter()
            .fold(String::new(), |mut acc, fragment| {
                acc.push_str(&fragment.grapheme);
                acc
            })
    }

    fn convert_string_to_content(line_str: &str) -> Vec<TextFragment> {
        let fragments = line_str
            .graphemes(true)
            .map(|grapheme| {
                let unicode_width = grapheme.width();
                let rendered_width = match unicode_width {
                    0 | 1 => GraphemeWidth::Half,
                    _ => GraphemeWidth::Full,
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
            line.convert_content_to_string(0..line.grapheme_count()),
            "helo"
        );
    }

    #[test]
    fn delete_grapheme_at_start_and_end() {
        let mut line = Line::from("abc");

        line.delete_grapheme(0); // remove 'a'
        assert_eq!(
            line.convert_content_to_string(0..line.grapheme_count()),
            "bc"
        );

        let mut line = Line::from("abc");
        line.delete_grapheme(2); // remove 'c' — left with ["a", "b"]
        assert_eq!(
            line.convert_content_to_string(0..line.grapheme_count()),
            "ab"
        );

        // On the same line, delete at index 0 removes 'a', leaving ['b']
        line.delete_grapheme(0);
        assert_eq!(
            line.convert_content_to_string(0..line.grapheme_count()),
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
            line.convert_content_to_string(0..line.grapheme_count()),
            "abc"
        );
    }

    #[test]
    fn insert_char_in_middle() {
        let mut line = Line::from("ac");
        line.insert_char(1, 'b');
        assert_eq!(
            line.convert_content_to_string(0..line.grapheme_count()),
            "abc"
        );
    }

    #[test]
    fn insert_char_at_end() {
        let mut line = Line::from("ab");
        line.insert_char(2, 'c');
        assert_eq!(
            line.convert_content_to_string(0..line.grapheme_count()),
            "abc"
        );
    }

    #[test]
    fn insert_char_into_empty_line() {
        let mut line = Line::from("");
        line.insert_char(0, 'x');
        assert_eq!(
            line.convert_content_to_string(0..line.grapheme_count()),
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
            line.convert_content_to_string(0..line.grapheme_count()),
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
            line.convert_content_to_string(0..line.grapheme_count()),
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
            line.convert_content_to_string(0..line.grapheme_count()),
            "abc"
        );
    }

    #[test]
    fn insert_char_preserves_existing_graphemes() {
        let mut line = Line::from("hello");
        line.insert_char(3, 'X');
        assert_eq!(
            line.convert_content_to_string(0..line.grapheme_count()),
            "helXlo"
        );

        // Verify remaining graphemes are intact
        line.insert_char(6, 'Y');
        assert_eq!(
            line.convert_content_to_string(0..line.grapheme_count()),
            "helXloY"
        );
    }

    #[test]
    fn insert_char_with_emoji() {
        let mut line = Line::from("ab");
        line.insert_char(1, '👍');
        let text = line.convert_content_to_string(0..line.grapheme_count());
        assert!(text.contains('👍'));
        assert_eq!(line.grapheme_count(), 3);
    }

    #[test]
    fn insert_char_at_beginning_of_emoji_line() {
        let mut line = Line::from("👍bc");
        line.insert_char(0, 'a');
        let text = line.convert_content_to_string(0..line.grapheme_count());
        assert!(text.starts_with('a'));
        assert_eq!(line.grapheme_count(), 4);
    }

    #[test]
    fn insert_char_between_two_emoji() {
        let mut line = Line::from("👍🎉");
        line.insert_char(1, 'x');
        let text = line.convert_content_to_string(0..line.grapheme_count());
        assert_eq!(text, "👍x🎉");
        assert_eq!(line.grapheme_count(), 3);
    }

    #[test]
    fn insert_char_repeatedly_at_same_position() {
        let mut line = Line::from("abc");
        for i in 0..5 {
            line.insert_char(1, char::from_digit(i, 10).unwrap());
        }
        let text = line.convert_content_to_string(0..line.grapheme_count());
        // Each insertion at index 1 pushes the previous inserted characters right
        assert_eq!(text, "a43210bc");
    }

    #[test]
    fn insert_char_into_line_with_empty_graphemes() {
        // Insert between two normal chars on any existing grapheme count
        let mut line = Line::from("hello");
        line.insert_char(0, 'H');
        assert_eq!(
            line.convert_content_to_string(0..line.grapheme_count()),
            "Hhello"
        );
    }
}
