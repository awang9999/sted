use crate::common::{
    constants::TAB_WIDTH_SPACES,
    textfragment::{GraphemeWidth, TextFragment},
};
use std::fmt;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// How a display segment of a line is highlighted, if at all. The view
/// renders each kind differently: plain segments in the terminal's
/// default colors, search matches in the match highlight, and the active
/// match — the one the caret sits on — in its own, stronger highlight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentHighlight {
    /// No highlight: drawn with the terminal's default colors.
    None,
    /// A search match that is not the active one.
    Match,
    /// The active search match, where the caret sits.
    Active,
}

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

    /// A line holding the single character `c`.
    pub fn from_char(c: char) -> Self {
        Self::from(&c.to_string())
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

    /// The index of the grapheme that contains byte `byte_index` of the
    /// line's text: the last grapheme starting at or before that byte.
    /// Indices past the end of the text yield the index of the last
    /// grapheme.
    fn grapheme_index_at_byte(&self, byte_index: usize) -> usize {
        self.fragments
            .iter()
            .take_while(|fragment| fragment.byte_index <= byte_index)
            .count()
            .saturating_sub(1)
    }

    /// The grapheme index of the first occurrence of `pattern` in the
    /// line's text, or `None` when the line does not contain it.
    pub fn find(&self, pattern: &str) -> Option<usize> {
        self.find_from(pattern, 0)
    }

    /// The grapheme index of the first occurrence of `pattern` at or
    /// after `grapheme_index`, or `None` when the line does not contain
    /// it there.
    ///
    /// Finding from a position is finding in the slice of the line's text
    /// that starts at that position: the pattern is located byte-wise
    /// with [`str::find`] in the slice and its slice-relative byte index
    /// is translated to the grapheme it falls in, so a match that starts
    /// inside a grapheme cluster reports that cluster. Positions past the
    /// end of the line yield `None`. The empty pattern matches at
    /// `grapheme_index`.
    pub fn find_from(&self, pattern: &str, grapheme_index: usize) -> Option<usize> {
        let start = self.byte_index_at(grapheme_index);
        let byte_index = self.text[start..].find(pattern)?;
        Some(self.grapheme_index_at_byte(start.saturating_add(byte_index)))
    }

    /// The grapheme index of the last occurrence of `pattern` in the
    /// line's text, or `None` when the line does not contain it.
    pub fn rfind(&self, pattern: &str) -> Option<usize> {
        self.rfind_before(pattern, self.grapheme_count())
    }

    /// The grapheme index of the last occurrence of `pattern` that ends
    /// at or before `grapheme_index`, or `None` when the line does not
    /// contain it there.
    ///
    /// The backwards counterpart of [`Line::find_from`]: finding backwards
    /// from a position is finding in the slice of the line's text that
    /// ends at that position: the pattern is located byte-wise with
    /// [`str::rfind`] in the slice and its byte index is translated to the
    /// grapheme it falls in. Positions past the end of the line yield the
    /// line's last occurrence.
    pub fn rfind_before(&self, pattern: &str, grapheme_index: usize) -> Option<usize> {
        let end = self.byte_index_at(grapheme_index);
        let byte_index = self.text[..end].rfind(pattern)?;
        Some(self.grapheme_index_at_byte(byte_index))
    }

    /// The grapheme span of every occurrence of `pattern` in this line's
    /// text, in document order, including overlapping occurrences. Each
    /// span starts at the grapheme the occurrence starts in and covers one
    /// grapheme per grapheme of the pattern — the same span the search
    /// treats as the match, so a highlight built from these spans is
    /// consistent with the search: every occurrence the search can reach
    /// is covered, and nothing else is. An empty pattern is not a search
    /// and yields no spans.
    pub fn match_spans(&self, pattern: &str) -> Vec<Range<usize>> {
        if pattern.is_empty() {
            return Vec::new();
        }
        let span = pattern.grapheme_indices(true).count();
        let mut spans = Vec::new();
        let mut search_from = 0;
        while let Some(relative) = self.text[search_from..].find(pattern) {
            let byte_index = search_from.saturating_add(relative);
            let start = self.grapheme_index_at_byte(byte_index);
            spans.push(start..start.saturating_add(span));
            // Step past the occurrence's first byte, so overlapping
            // occurrences are found too. A match can only start at a char
            // boundary, so a step that lands inside a multi-byte grapheme
            // is skipped to the boundary past it.
            let mut next = byte_index.saturating_add(1);
            while next < self.text.len() && !self.text.is_char_boundary(next) {
                next = next.saturating_add(1);
            }
            search_from = next;
        }
        spans
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
                TextFragment {
                    byte_index,
                    rendered_width,
                    replacement,
                }
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
                merged.push(TextFragment {
                    byte_index: first,
                    rendered_width,
                    replacement,
                });
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
    // Only the test suite renders a line through `get` so far; the editor
    // draws its segments directly via [`Line::display_segments`].
    #[allow(dead_code)]
    pub fn get(&self, range: &Range<usize>) -> String {
        self.display_segments(range, None, None)
            .into_iter()
            .map(|(text, _)| text)
            .collect()
    }

    /// The visible portion of the line as display segments: the same
    /// truncation and ellipses as [`Line::get`], but with the graphemes of
    /// the open search marked so the caller can render them differently.
    /// A grapheme covered by `active` — the grapheme span of the active
    /// match, where the caret sits — is [`SegmentHighlight::Active`], a
    /// grapheme covered by any span of `matches` — the grapheme spans of
    /// the line's occurrences of the search pattern — is
    /// [`SegmentHighlight::Match`], and every other grapheme is
    /// [`SegmentHighlight::None`].
    ///
    /// A grapheme straddling an edge of `range` is not drawn; an ellipsis
    /// marks each edge that hides content.
    pub fn display_segments(
        &self,
        range: &Range<usize>,
        matches: Option<&[Range<usize>]>,
        active: Option<&Range<usize>>,
    ) -> Vec<(String, SegmentHighlight)> {
        let mut segments: Vec<(String, SegmentHighlight)> = Vec::new();
        if self.fragments.is_empty() {
            return segments;
        }

        let mut idx = 0; // track index of grapheme vector

        // Skip past graphemes before the visible range: `idx` is the first
        // grapheme starting at or inside the range.
        while idx < self.fragments.len() && self.width_until(idx) < range.start {
            idx = idx.saturating_add(1);
        }

        // Truncation at range.start? The previous grapheme hides content
        // only when it starts before the range AND still extends past it;
        // a grapheme ending exactly at the edge is fully outside.
        if idx > 0
            && self.width_until(idx.saturating_sub(1)) < range.start
            && self.width_until(idx) > range.start
        {
            segments.push((String::from('…'), SegmentHighlight::None));
        }

        // Add graphemes within the visible range, marking the ones a
        // search highlight covers (the active match takes precedence over
        // a plain match); consecutive graphemes with the same marking
        // join one segment.
        while idx < self.fragments.len() && self.width_until(idx.saturating_add(1)) <= range.end {
            let highlight = if active.is_some_and(|h| h.contains(&idx)) {
                SegmentHighlight::Active
            } else if matches.is_some_and(|m| m.iter().any(|h| h.contains(&idx))) {
                SegmentHighlight::Match
            } else {
                SegmentHighlight::None
            };
            match segments.last_mut() {
                Some((text, marked)) if *marked == highlight => {
                    text.push_str(self.display_text_of(idx));
                }
                _ => segments.push((self.display_text_of(idx).to_string(), highlight)),
            }
            idx = idx.saturating_add(1);
        }

        // Truncation at range.end? Any grapheme left undrawn means the
        // range cut off content, whether it straddled the edge or starts
        // past it.
        if idx < self.fragments.len() {
            segments.push((String::from('…'), SegmentHighlight::None));
        }

        segments
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
            .map(|fragment| fragment.rendered_width.columns())
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
        assert_eq!(line.get(&(0..10)), "a    b");
    }

    #[test]
    fn get_keeps_the_grapheme_that_starts_at_the_left_edge() {
        let line = Line::from("abcdef");
        assert_eq!(line.get(&(1..7)), "bcdef");
        assert_eq!(line.get(&(2..7)), "cdef");
    }

    #[test]
    fn get_ellipses_a_grapheme_straddling_the_left_edge_and_keeps_the_rest() {
        // Columns: a=0, b=1, 中=2..4, c=4, d=5, e=6.
        let line = Line::from("ab中cde");
        // 中 straddles column 3, so it is elided — but c..e stay.
        assert_eq!(line.get(&(3..10)), "…cde");
        // 中 ends exactly at column 4: it is fully outside, no ellipsis.
        assert_eq!(line.get(&(4..10)), "cde");
    }

    #[test]
    fn get_ellipses_content_hidden_by_the_right_edge() {
        let line = Line::from("abcdef");
        // f straddles column 5: it is cut off, marked with an ellipsis.
        assert_eq!(line.get(&(0..5)), "abcde…");
        // d starts exactly at column 3: still hidden, still elided.
        assert_eq!(line.get(&(0..3)), "abc…");
        // Nothing hidden past the edge: no ellipsis.
        assert_eq!(line.get(&(0..6)), "abcdef");
    }

    fn segments_as_pairs(segments: &[(String, SegmentHighlight)]) -> Vec<(&str, SegmentHighlight)> {
        segments
            .iter()
            .map(|(text, hl)| (text.as_str(), *hl))
            .collect()
    }

    #[test]
    fn display_segments_without_search_highlights_match_get() {
        let line = Line::from("abcdef");
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(1..7), None, None)),
            vec![("bcdef", SegmentHighlight::None)]
        );
        // A straddling grapheme at the left edge is elided; the ellipsis
        // joins the unmarked text that follows it.
        let wide = Line::from("ab中cde");
        assert_eq!(
            segments_as_pairs(&wide.display_segments(&(3..10), None, None)),
            vec![("…cde", SegmentHighlight::None)]
        );
        // 中 starts exactly at column 2: no left ellipsis; d starts
        // exactly at column 5: hidden, elided. The right ellipsis stays
        // its own segment.
        assert_eq!(
            segments_as_pairs(&wide.display_segments(&(2..5), None, None)),
            vec![
                ("中c", SegmentHighlight::None),
                ("…", SegmentHighlight::None)
            ]
        );
    }

    #[test]
    fn display_segments_marks_the_graphemes_of_a_match() {
        let line = Line::from("hello world");
        let world = 6..11;
        let hello = 0..5;
        let rld = 8..100;
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(0..20), Some(&[world]), None)),
            vec![
                ("hello ", SegmentHighlight::None),
                ("world", SegmentHighlight::Match)
            ]
        );
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(0..20), Some(&[hello]), None)),
            vec![
                ("hello", SegmentHighlight::Match),
                (" world", SegmentHighlight::None)
            ]
        );
        // A match past the end of the line marks what it covers.
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(0..20), Some(&[rld]), None)),
            vec![
                ("hello wo", SegmentHighlight::None),
                ("rld", SegmentHighlight::Match)
            ]
        );
    }

    #[test]
    fn display_segments_marks_every_match_of_the_line() {
        let line = Line::from("hello world hello");
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(0..20), Some(&[0..5, 12..17]), None)),
            vec![
                ("hello", SegmentHighlight::Match),
                (" world ", SegmentHighlight::None),
                ("hello", SegmentHighlight::Match)
            ]
        );
    }

    #[test]
    fn display_segments_marks_the_active_match_over_a_plain_match() {
        let line = Line::from("hello world hello");
        // The caret sits on the second "hello": it is the active match,
        // the first one stays a plain match.
        assert_eq!(
            segments_as_pairs(&line.display_segments(
                &(0..20),
                Some(&[0..5, 12..17]),
                Some(&(12..17))
            )),
            vec![
                ("hello", SegmentHighlight::Match),
                (" world ", SegmentHighlight::None),
                ("hello", SegmentHighlight::Active)
            ]
        );
        // An active match without other matches marks only itself.
        let other = Line::from("hello world");
        assert_eq!(
            segments_as_pairs(&other.display_segments(&(0..20), None, Some(&(6..11)))),
            vec![
                ("hello ", SegmentHighlight::None),
                ("world", SegmentHighlight::Active)
            ]
        );
    }

    #[test]
    fn display_segments_clips_the_matches_to_the_visible_range() {
        // Columns: h=0..e=4, space=5, w=6..d=10.
        let line = Line::from("hello world");
        let world = 6..11;
        let head = 0..7;
        let outside = 0..3;
        // The window [3..9) shows "lo wor": the match 6..11 reaches
        // only w, o, r before the ellipsis cuts the rest.
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(3..9), Some(&[world]), None)),
            vec![
                ("lo ", SegmentHighlight::None),
                ("wor", SegmentHighlight::Match),
                ("…", SegmentHighlight::None)
            ]
        );
        // A match that starts left of the window is clipped at it:
        // it still covers l, o, space and w, which are visible.
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(3..20), Some(&[head]), None)),
            vec![
                ("lo w", SegmentHighlight::Match),
                ("orld", SegmentHighlight::None)
            ]
        );
        // A match fully outside the window leaves every segment plain.
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(3..9), Some(&[outside]), None)),
            vec![
                ("lo wor", SegmentHighlight::None),
                ("…", SegmentHighlight::None)
            ]
        );
    }

    #[test]
    fn display_segments_of_an_empty_line_is_empty() {
        assert_eq!(
            Line::from("").display_segments(&(0..10), None, None),
            Vec::<(String, SegmentHighlight)>::new()
        );
    }

    #[test]
    fn match_spans_finds_every_occurrence_of_the_pattern() {
        let line = Line::from("hello world hello");
        assert_eq!(line.match_spans("hello"), vec![0..5, 12..17]);
        // Single and no occurrences.
        assert_eq!(line.match_spans("world"), vec![6..11]);
        assert_eq!(line.match_spans("zzz"), Vec::<Range<usize>>::new());
    }

    #[test]
    fn match_spans_reports_grapheme_indices_not_bytes() {
        // 'é' is two bytes in UTF-8 but one grapheme, so the second
        // "café" of "café café" starts at grapheme 5, not byte 6.
        let line = Line::from("café café");
        assert_eq!(line.match_spans("café"), vec![0..4, 5..9]);
        assert_eq!(line.match_spans("latte"), Vec::<Range<usize>>::new());
    }

    #[test]
    fn match_spans_finds_overlapping_occurrences() {
        let line = Line::from("aaa");
        assert_eq!(line.match_spans("aa"), vec![0..2, 1..3]);
    }

    #[test]
    fn match_spans_reports_the_grapheme_a_match_starts_in() {
        // 'é' is one grapheme at index 3, and "fé" starts at 'f' (2) and
        // covers the 'é' grapheme too.
        let line = Line::from("café");
        assert_eq!(line.match_spans("é"), vec![3..4]);
        assert_eq!(line.match_spans("fé"), vec![2..4]);
    }

    #[test]
    fn match_spans_of_an_empty_pattern_is_empty() {
        assert_eq!(
            Line::from("abc").match_spans(""),
            Vec::<Range<usize>>::new()
        );
    }

    #[test]
    fn match_spans_on_an_empty_line_is_empty() {
        assert_eq!(Line::from("").match_spans("a"), Vec::<Range<usize>>::new());
    }

    #[test]
    fn match_spans_counts_a_tab_as_one_grapheme() {
        let line = Line::from("a\tb c");
        assert_eq!(line.match_spans("\t"), vec![1..2]);
        assert_eq!(line.match_spans("b c"), vec![2..5]);
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
    fn display_writes_the_verbatim_line_content() {
        let line = Line::from("café");
        assert_eq!(line.to_string(), "café");
    }

    #[test]
    fn find_returns_the_grapheme_index_of_the_match() {
        let line = Line::from("hello world");
        assert_eq!(line.find("world"), Some(6));
        assert_eq!(line.find("hello"), Some(0));
        assert_eq!(line.find("lo wo"), Some(3));
    }

    #[test]
    fn find_of_a_multibyte_pattern_translates_the_byte_index() {
        // 'é' is two bytes in UTF-8, so "café" spans five bytes while the
        // grapheme index of 'é' is 3.
        let line = Line::from("café");
        assert_eq!(line.find("é"), Some(3));
        assert_eq!(line.find("café"), Some(0));
        // "fé" starts at byte 2, the byte index of 'f'.
        assert_eq!(line.find("fé"), Some(2));
    }

    #[test]
    fn find_reports_the_grapheme_containing_a_match_inside_a_cluster() {
        // The combining mark after 'f' forms one grapheme with it, at
        // index 2 of "café" (f + U+0301 + e).
        let line = Line::from("caf\u{301}e");
        assert_eq!(line.find("\u{301}"), Some(2));
        assert_eq!(line.find("caf"), Some(0));
        assert_eq!(line.find("e"), Some(3));
    }

    #[test]
    fn find_on_a_line_with_tabs_counts_tabs_as_graphemes() {
        let line = Line::from("a\tb c");
        assert_eq!(line.find("b"), Some(2));
        assert_eq!(line.find("c"), Some(4));
    }

    #[test]
    fn find_without_a_match_is_none() {
        let line = Line::from("abc");
        assert_eq!(line.find("d"), None);
        assert_eq!(line.find("abcd"), None);
    }

    #[test]
    fn find_on_an_empty_line_is_none() {
        let line = Line::from("");
        assert_eq!(line.find("a"), None);
    }

    #[test]
    fn find_of_an_empty_pattern_matches_at_the_start_of_the_line() {
        assert_eq!(Line::from("abc").find(""), Some(0));
        assert_eq!(Line::from("").find(""), Some(0));
    }

    #[test]
    fn find_from_skips_matches_before_the_position() {
        let line = Line::from("hello world");
        assert_eq!(line.find_from("hello", 1), None);
        assert_eq!(line.find_from("world", 6), Some(6));
        // The 'o's at 4 and 5 are before the position.
        assert_eq!(line.find_from("o", 6), Some(7));
    }

    #[test]
    fn find_from_finds_a_match_that_starts_at_the_position() {
        let line = Line::from("hello world");
        assert_eq!(line.find_from("lo world", 3), Some(3));
        assert_eq!(line.find_from("o", 4), Some(4));
    }

    #[test]
    fn find_from_past_the_end_of_the_line_is_none() {
        let line = Line::from("abc");
        assert_eq!(line.find_from("abc", 1), None);
        assert_eq!(line.find_from("bc", 2), None);
        assert_eq!(line.find_from("c", 3), None);
    }

    #[test]
    fn find_from_of_zero_is_the_plain_find() {
        let line = Line::from("café latte");
        assert_eq!(line.find_from("latte", 0), line.find("latte"));
        assert_eq!(line.find_from("é", 0), line.find("é"));
        assert_eq!(line.find_from("zzz", 0), line.find("zzz"));
    }

    #[test]
    fn rfind_returns_the_grapheme_index_of_the_last_match() {
        let line = Line::from("hello world hello");
        assert_eq!(line.rfind("hello"), Some(12));
        assert_eq!(line.rfind("world"), Some(6));
        assert_eq!(line.rfind("o"), Some(16));
    }

    #[test]
    fn rfind_of_a_multibyte_pattern_translates_the_byte_index() {
        // 'é' is two bytes in UTF-8, so the last "café" of "café café"
        // starts at grapheme 5, byte 6.
        let line = Line::from("café café");
        assert_eq!(line.rfind("café"), Some(5));
        assert_eq!(line.rfind("é"), Some(8));
    }

    #[test]
    fn rfind_without_a_match_is_none() {
        let line = Line::from("abc");
        assert_eq!(line.rfind("d"), None);
    }

    #[test]
    fn rfind_before_skips_matches_that_end_after_the_position() {
        let line = Line::from("hello world hello");
        // The last "hello" ends at column 17, past the position.
        assert_eq!(line.rfind_before("hello", 16), Some(0));
        // A match ending exactly at the position counts.
        assert_eq!(line.rfind_before("hello", 17), Some(12));
        // Nothing before the start.
        assert_eq!(line.rfind_before("hello", 0), None);
    }

    #[test]
    fn rfind_before_past_the_end_of_the_line_is_the_plain_rfind() {
        let line = Line::from("café latte");
        assert_eq!(
            line.rfind_before("latte", line.grapheme_count()),
            line.rfind("latte")
        );
        assert_eq!(line.rfind_before("latte", 100), line.rfind("latte"));
        assert_eq!(line.rfind_before("zzz", 100), line.rfind("zzz"));
    }

    #[test]
    fn grapheme_index_at_byte_yields_the_grapheme_containing_the_byte() {
        // "caféx": byte indices 0, 1, 2, 3 ('é' spans 3..5), 5.
        let line = Line::from("caféx");
        assert_eq!(line.grapheme_index_at_byte(0), 0);
        assert_eq!(line.grapheme_index_at_byte(3), 3);
        // Byte 4 is the second byte of 'é': it falls in grapheme 3.
        assert_eq!(line.grapheme_index_at_byte(4), 3);
        assert_eq!(line.grapheme_index_at_byte(5), 4);
        // Past the end of the text: the last grapheme.
        assert_eq!(line.grapheme_index_at_byte(100), 4);
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
        assert_eq!(line.to_string(), "aXb");
        assert_eq!(byte_indices(&line), vec![0, 1, 2]);
    }

    #[test]
    fn insert_multibyte_char_shifts_byte_indices_by_its_utf8_length() {
        let mut line = Line::from("ab");
        line.insert_char(1, 'é');
        assert_eq!(line.to_string(), "aéb");
        // 'a' (1 byte) + 'é' (2 bytes) → 'b' starts at byte 3.
        assert_eq!(line.fragments[2].byte_index, 3);
    }

    #[test]
    fn insert_char_at_end_extends_the_text() {
        let mut line = Line::from("café");
        line.insert_char(4, 'x');
        assert_eq!(line.to_string(), "caféx");
        assert_eq!(line.fragments[4].byte_index, 5);
    }

    #[test]
    fn delete_grapheme_rebases_the_byte_indices_after_the_gap() {
        let mut line = Line::from("café");
        line.delete_grapheme(0); // remove 'c'
        assert_eq!(line.to_string(), "afé");
        assert_eq!(byte_indices(&line), vec![0, 1, 2]);
    }

    #[test]
    fn split_at_keeps_byte_indices_consistent_in_both_halves() {
        let mut line = Line::from("café");
        let after = line.split_at(2);
        assert_eq!(line.to_string(), "ca");
        assert_eq!(after.to_string(), "fé");
        assert_eq!(line.fragments[1].byte_index, 1);
        assert_eq!(after.fragments[1].byte_index, 1); // 'é' inside "fé"
    }

    #[test]
    fn appending_a_line_keeps_byte_indices_consistent() {
        let mut line = Line::from("ca");
        line.append(&Line::from("fé"));
        assert_eq!(line.to_string(), "café");
        assert_eq!(byte_indices(&line), vec![0, 1, 2, 3]);
    }

    #[test]
    fn appending_to_an_empty_line_yields_the_other_line() {
        let mut line = Line::from("");
        line.append(&Line::from("fé"));
        assert_eq!(line.to_string(), "fé");
        assert_eq!(line.grapheme_count(), 2);
        assert_eq!(byte_indices(&line), vec![0, 1]);
    }

    #[test]
    fn inserting_a_combining_mark_joins_the_previous_grapheme() {
        let mut line = Line::from("cafe");
        line.insert_char(3, '\u{301}'); // acute accent after 'f'
        assert_eq!(line.to_string(), "caf\u{301}e");
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
        assert_eq!(line.to_string(), "cae");
        assert_eq!(line.grapheme_count(), 3);
        assert_eq!(byte_indices(&line), vec![0, 1, 2]);
    }

    #[test]
    fn inserting_a_zwj_joins_the_neighbouring_emoji() {
        let mut line = Line::from("👍🎉");
        line.insert_char(1, '\u{200d}');
        assert_eq!(line.to_string(), "👍\u{200d}🎉");
        // The ZWJ joins the two emoji into one merged fragment.
        assert_eq!(line.grapheme_count(), 1);
        assert_eq!(line.fragments[0].byte_index, 0);
    }
}
