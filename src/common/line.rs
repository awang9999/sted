use crate::common::{
    constants::TAB_WIDTH_SPACES,
    style::{Style, LineStyle},
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
#[derive(Clone, Debug)]
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

    /// Whether the grapheme at `grapheme_index` is one of the characters
    /// a number may be built from: an ASCII digit, a hex digit (A-F,
    /// a-f), a dot, a lowercase `e` (scientific notation), a literal
    /// prefix letter (`x`, `b`, `o`, either case), or an underscore (a
    /// visual separator). A multi-char cluster like a keycap emoji is
    /// none of them.
    fn is_number_char(&self, grapheme_index: usize) -> bool {
        let text = self.display_text_of(grapheme_index);
        text.len() == 1
            && matches!(
                text.chars().next(),
                Some(c)
                    if c.is_ascii_hexdigit()
                        || c == '.'
                        || c == 'x'
                        || c == 'X'
                        || c == 'b'
                        || c == 'B'
                        || c == 'o'
                        || c == 'O'
                        || c == '_'
            )
    }

    /// Whether the grapheme at `grapheme_index` is a word character: an
    /// ASCII identifier character — a letter, a digit, or an underscore.
    /// A multi-char cluster like a keycap emoji is not one.
    fn is_word_char(&self, grapheme_index: usize) -> bool {
        let text = self.display_text_of(grapheme_index);
        text.len() == 1
            && matches!(
                text.chars().next(),
                Some(c) if c.is_ascii_alphanumeric() || c == '_'
            )
    }

    /// The grapheme spans of the line's numbers, in the order they occur
    /// in the line. A number is an integer (a run of ASCII digits), a
    /// float (exactly one dot, not at the start or the end), a
    /// scientific number (an integer or a float, a lowercase `e`, then a
    /// run of digits), a hexadecimal literal (`0x` or `0X`, then hex
    /// digits), a binary literal (`0b` or `0B`, then 0s and 1s), or an
    /// octal literal (`0o` or `0O`, then 0-7) — and each of those may
    /// use underscores as visual separators, a separator never at the
    /// start or the end of the number. Concretely, a number is one
    /// maximal run of the characters it may be built from (digits, hex
    /// digits, dots, `e`s, prefix letters, underscores) that forms a
    /// number under those rules; a run that does not is not a number —
    /// so the `123` of `abc123def` is not highlighted on its own.
    /// The line knows where the runs are; the styling of the spans is
    /// decided by the structure that owns the buffer's styling.
    pub fn number_spans(&self) -> Vec<Range<usize>> {
        let mut runs = Vec::new();
        let mut run_start: Option<usize> = None;
        for index in 0..self.fragments.len() {
            if self.is_number_char(index) {
                run_start.get_or_insert(index);
            } else if let Some(start) = run_start.take() {
                runs.push(start..index);
            }
        }
        if let Some(start) = run_start {
            runs.push(start..self.fragments.len());
        }
        runs
            .into_iter()
            .filter(|run| is_number(self.run_text(run.start, run.end)))
            .collect()
    }

    /// Whether the grapheme at `grapheme_index` is the quote that opens
    /// and closes a character literal. A multi-char cluster like a
    /// keycap emoji is not one.
    pub(crate) fn is_quote(&self, grapheme_index: usize) -> bool {
        let text = self.display_text_of(grapheme_index);
        text == "'"
    }

    /// Whether the grapheme at `grapheme_index` opens a comment marker:
    /// the grapheme and the one after it are both forward slashes — the
    /// `//` of a single-line comment. A multi-char cluster like a keycap
    /// emoji is no slash, and an index at or past the penultimate
    /// grapheme leaves no grapheme for the second slash.
    pub(crate) fn starts_comment(&self, grapheme_index: usize) -> bool {
        let text = self.display_text_of(grapheme_index);
        let next = grapheme_index.saturating_add(1);
        text == "/"
            && next < self.fragments.len()
            && self.display_text_of(next) == "/"
    }

    /// Whether the grapheme at `grapheme_index` opens a multi-line
    /// comment: the grapheme is a forward slash and the one after it is
    /// an asterisk — the `/*` of a block comment. A multi-char cluster
    /// like a keycap emoji is no slash, and an index at or past the
    /// penultimate grapheme leaves no grapheme for the asterisk.
    pub(crate) fn starts_ml_comment(&self, grapheme_index: usize) -> bool {
        let text = self.display_text_of(grapheme_index);
        let next = grapheme_index.saturating_add(1);
        text == "/"
            && next < self.fragments.len()
            && self.display_text_of(next) == "*"
    }

    /// Whether the grapheme at `grapheme_index` closes a multi-line
    /// comment: the grapheme is an asterisk and the one after it is a
    /// forward slash — the `*/` of a block comment.
    pub(crate) fn closes_ml_comment(&self, grapheme_index: usize) -> bool {
        let text = self.display_text_of(grapheme_index);
        let next = grapheme_index.saturating_add(1);
        text == "*"
            && next < self.fragments.len()
            && self.display_text_of(next) == "/"
    }

    /// The grapheme spans of the line's words, in the order they occur
    /// in the line. A word is one maximal run of ASCII identifier
    /// characters (`[A-Za-z0-9_]`), and every run is reported, whatever
    /// it forms: the line knows where the runs are; the decision of
    /// which of them carries a style — a keyword, a type, a literal —
    /// is made by the structure that owns the buffer's styling.
    ///
    /// The character class is deliberately ASCII-only: Rust identifiers
    /// may be Unicode, but every word the styling lists contain is
    /// ASCII, so a Unicode identifier can never be a whole-word match,
    /// and an ASCII maximal-run matcher is sufficient.
    pub fn word_spans(&self) -> Vec<Range<usize>> {
        let mut runs = Vec::new();
        let mut run_start: Option<usize> = None;
        for index in 0..self.fragments.len() {
            if self.is_word_char(index) {
                run_start.get_or_insert(index);
            } else if let Some(start) = run_start.take() {
                runs.push(start..index);
            }
        }
        if let Some(start) = run_start {
            runs.push(start..self.fragments.len());
        }
        runs
    }

    /// The grapheme spans of the line's lifetime specifiers, in the
    /// order they occur in the line. A lifetime specifier is a quote
    /// directly followed by at least one ASCII identifier character —
    /// a letter, a digit, or an underscore — and the span covers the
    /// quote plus the maximal such run that follows it: `'a_b2` is
    /// five graphemes, `'static` is seven, `'_` is two. A quote at the
    /// end of the line, or one followed by anything else — a space,
    /// punctuation, a non-ASCII character — starts no run, so its
    /// quote stays unreported.
    ///
    /// The character class is deliberately ASCII-only, like the word
    /// matcher: a lifetime name is one the compiler accepts, and every
    /// one worth styling is ASCII, so `'é` is no lifetime.
    ///
    /// The scan knows every quote on the line; deciding which of them
    /// belong to a character literal — and therefore are not the start
    /// of a lifetime — is left to the structure that owns the buffer's
    /// styling, which styles the character literals first.
    pub fn lifetime_spans(&self) -> Vec<Range<usize>> {
        let mut spans = Vec::new();
        let len = self.fragments.len();
        let mut index = 0;
        while index < len {
            if self.is_quote(index) {
                let after = index.saturating_add(1);
                if after < len && self.is_word_char(after) {
                    let mut end = after;
                    while end < len && self.is_word_char(end) {
                        end = end.saturating_add(1);
                    }
                    spans.push(index..end);
                    index = end;
                    continue;
                }
            }
            index = index.saturating_add(1);
        }
        spans
    }

    /// The grapheme index just past the closing quote of the character
    /// literal that opens at grapheme `open` — the exclusive end of the
    /// whole literal: its opening quote, its interior, and its closing
    /// quote — or `None` when the grapheme at `open` is not a quote, or
    /// when the line carries no closing quote for it.
    ///
    /// The scan works in grapheme space: it starts at the grapheme
    /// after the opening quote, skips the grapheme a backslash directly
    /// after the opening quote escapes, and takes the next quote it
    /// finds as the closer. Two deliberate simplifications:
    ///
    /// - at most one escape, sitting directly after the opening quote —
    ///   the form of a quoted quote. A backslash anywhere else is no
    ///   escape: the scan stops at the very next quote it finds;
    /// - the quotes pair naively, left to right: the next quote on the
    ///   line that opens no lifetime run closes the literal whatever
    ///   it is, so an apostrophe in prose pairs with the quote that
    ///   follows it;
    /// - a quote that opens a lifetime run — an ASCII letter, digit,
    ///   or underscore directly after it, with no closing quote for
    ///   that run — never closes a literal: it belongs to the lifetime
    ///   highlighting, which runs after the character highlighting.
    ///   A quote whose run is itself closed by a quote (`'test'`) still
    ///   closes: it is the opening of another character literal, not a
    ///   lifetime. Without this, in
    ///   `fn f<'a>(s: &'a str) -> &'static str` the first quote of the
    ///   generics would pair with the quote of the second `'a` and
    ///   swallow the stretch between them as one character literal.
    ///
    /// For a literal whose interior is pure ASCII, the returned index
    /// equals the byte index just past the closing quote. An `open` past
    /// the end of the line yields `None`.
    pub fn char_literal_end(&self, open: usize) -> Option<usize> {
        if open >= self.fragments.len() || !self.is_quote(open) {
            return None;
        }
        let mut index = open.saturating_add(1);
        // A backslash directly after the opening quote escapes the
        // grapheme it precedes — the one escape the literal may carry.
        if index < self.fragments.len() && self.display_text_of(index) == "\\" {
            index = index.saturating_add(2);
        }
        while index < self.fragments.len() {
            if self.is_quote(index) && !self.starts_lifetime_run(index) {
                return Some(index.saturating_add(1));
            }
            index = index.saturating_add(1);
        }
        None
    }

    /// Whether the quote at `grapheme_index` opens a lifetime run: an
    /// ASCII letter, digit, or underscore directly after it, with the
    /// run NOT immediately closed by a quote. Used by
    /// [`Line::char_literal_end`] so such a quote is never taken as a
    /// closer — it belongs to the lifetime highlighting. A quote whose
    /// run is immediately followed by a quote (`'test'`) is no
    /// lifetime: it opens another character literal. A multi-char
    /// cluster like a keycap emoji is no run character.
    pub(crate) fn starts_lifetime_run(&self, grapheme_index: usize) -> bool {
        let after = grapheme_index.saturating_add(1);
        if after >= self.fragments.len() || !self.is_word_char(after) {
            return false;
        }
        let mut end = after;
        while end < self.fragments.len() && self.is_word_char(end) {
            end = end.saturating_add(1);
        }
        // The run is a lifetime only if the character right after it
        // is not a quote: a quote immediately after the run means the
        // run is the interior of a character literal, not a lifetime
        // specifier.
        end >= self.fragments.len() || !self.is_quote(end)
    }

    /// The text of the grapheme run from `start` to (exclusive) `end`
    /// as a slice of the line's text: every grapheme of a run of
    /// single-character graphemes — a number run, a word run — is one
    /// character, so the run is a contiguous slice of the text.
    pub(crate) fn run_text(&self, start: usize, end: usize) -> &str {
        let from = self.fragments[start].byte_index;
        let to = if end < self.fragments.len() {
            self.fragments[end].byte_index
        } else {
            self.text.len()
        };
        &self.text[from..to]
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
        self.display_segments(range, &LineStyle::new())
            .into_iter()
            .map(|(text, _)| text)
            .collect()
    }

    /// The visible portion of the line as display segments: the same
    /// truncation and ellipses as [`Line::get`], but each segment also
    /// carries the style its graphemes carry in `style` — the line's
    /// styling information, resolved grapheme by grapheme.
    ///
    /// `style` is the line's share of the buffer's styling: the
    /// structure that owns it stores the styling per line and hands it
    /// to the line when the line builds its annotated string, so this
    /// method only resolves and merges — it knows nothing about what
    /// produced the styling.
    ///
    /// A grapheme straddling an edge of `range` is not drawn; an ellipsis
    /// marks each edge that hides content.
    pub fn display_segments(
        &self,
        range: &Range<usize>,
        style: &LineStyle,
    ) -> Vec<(String, Style)> {
        let mut segments: Vec<(String, Style)> = Vec::new();
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
            segments.push((String::from('…'), Style::None));
        }

        // Add graphemes within the visible range, resolving each
        // grapheme's style from the line's styling information;
        // consecutive graphemes with the same style join one segment.
        while idx < self.fragments.len() && self.width_until(idx.saturating_add(1)) <= range.end {
            let grapheme_style = style.at(idx);
            match segments.last_mut() {
                Some((text, marked)) if *marked == grapheme_style => {
                    text.push_str(self.display_text_of(idx));
                }
                _ => segments.push((self.display_text_of(idx).to_string(), grapheme_style)),
            }
            idx = idx.saturating_add(1);
        }

        // Truncation at range.end? Any grapheme left undrawn means the
        // range cut off content, whether it straddled the edge or starts
        // past it.
        if idx < self.fragments.len() {
            segments.push((String::from('…'), Style::None));
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

/// Whether `run` — a maximal run of the characters a number may be
/// built from — is a number: an integer (a run of ASCII digits), a float
/// (exactly one dot, not at the start or the end), a scientific number
/// (an integer or a float, a lowercase `e`, then a run of digits), a
/// hexadecimal literal (`0x` or `0X`, then one or more hex digits), a
/// binary literal (`0b` or `0B`, then one or more 0s and 1s), or an
/// octal literal (`0o` or `0O`, then one or more 0-7); underscores may
/// separate the characters anywhere but the start and the end of the
/// run, so they are stripped before the check.
fn is_number(run: &str) -> bool {
    if run.starts_with('_') || run.ends_with('_') {
        return false;
    }
    let stripped: String = run.chars().filter(|&c| c != '_').collect();
    // Every character of a run is ASCII, so a two-byte slice is a valid
    // prefix probe: a longer run hands over its first two characters,
    // a shorter one its whole self, which matches no prefix below.
    match &stripped[..stripped.len().min(2)] {
        "0x" | "0X" => is_hex_digits(&stripped[2..]),
        "0b" | "0B" => is_binary_digits(&stripped[2..]),
        "0o" | "0O" => is_octal_digits(&stripped[2..]),
        _ => is_plain_number(&stripped),
    }
}

/// Whether `text` is one or more hexadecimal digits (0-9, A-F, a-f).
fn is_hex_digits(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// Whether `text` is one or more binary digits (0 or 1).
fn is_binary_digits(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c == '0' || c == '1')
}

/// Whether `text` is one or more octal digits (0-7).
fn is_octal_digits(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| ('0'..='7').contains(&c))
}

/// Whether `stripped` — the underscore-free form of a run that carries
/// no literal prefix — is an integer or a float, or a scientific
/// number.
fn is_plain_number(stripped: &str) -> bool {
    match stripped.find('e') {
        Some(at_e) if !stripped[at_e.saturating_add(1)..].contains('e') => {
            is_int_or_float(&stripped[..at_e]) && is_int(&stripped[at_e.saturating_add(1)..])
        }
        None => is_int_or_float(stripped),
        Some(_) => false,
    }
}

/// Whether `text` is an integer (one or more ASCII digits) or a float
/// (exactly one dot, with a run of digits on each side of it).
fn is_int_or_float(text: &str) -> bool {
    match text.find('.') {
        Some(at_dot) if !text[at_dot.saturating_add(1)..].contains('.') => {
            is_int(&text[..at_dot]) && is_int(&text[at_dot.saturating_add(1)..])
        }
        None => is_int(text),
        Some(_) => false,
    }
}

/// Whether `text` is one or more ASCII digits.
fn is_int(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::style::Styles;
    use crate::common::types::FileType;

    fn byte_indices(line: &Line) -> Vec<usize> {
        line.fragments.iter().map(|f| f.byte_index).collect()
    }

    /// The base styling of `line` of a Rust file — its digit spans — as
    /// stored by the structure that owns the buffer's styling.
    fn base_style(line: &Line) -> LineStyle {
        Styles::for_lines(FileType::Rust, std::slice::from_ref(line))
            .line(0)
            .expect("the slice has the line")
            .clone()
    }

    /// The styling of `line` with the given match and active spans
    /// layered over its base styling: the same layering the view applies
    /// when it composes a line's styling for rendering.
    fn layered(
        line: &Line,
        matches: Option<&[Range<usize>]>,
        active: Option<Range<usize>>,
    ) -> LineStyle {
        let mut style = base_style(line);
        if let Some(spans) = matches {
            for span in spans {
                style.add(span.clone(), Style::Match);
            }
        }
        if let Some(span) = active {
            style.add(span, Style::Active);
        }
        style
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

    fn segments_as_pairs(segments: &[(String, Style)]) -> Vec<(&str, Style)> {
        segments
            .iter()
            .map(|(text, hl)| (text.as_str(), *hl))
            .collect()
    }

    #[test]
    fn display_segments_without_any_style_match_get() {
        let line = Line::from("abcdef");
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(1..7), &LineStyle::new())),
            vec![("bcdef", Style::None)]
        );
        // A straddling grapheme at the left edge is elided; the ellipsis
        // joins the unmarked text that follows it.
        let wide = Line::from("ab中cde");
        assert_eq!(
            segments_as_pairs(&wide.display_segments(&(3..10), &LineStyle::new())),
            vec![("…cde", Style::None)]
        );
        // 中 starts exactly at column 2: no left ellipsis; d starts
        // exactly at column 5: hidden, elided. The right ellipsis stays
        // its own segment.
        assert_eq!(
            segments_as_pairs(&wide.display_segments(&(2..5), &LineStyle::new())),
            vec![("中c", Style::None), ("…", Style::None)]
        );
    }

    #[test]
    fn display_segments_marks_the_graphemes_of_a_match() {
        let line = Line::from("hello world");
        let world = 6..11;
        let hello = 0..5;
        let rld = 8..100;
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(0..20), &layered(&line, Some(&[world]), None))),
            vec![("hello ", Style::None), ("world", Style::Match)]
        );
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(0..20), &layered(&line, Some(&[hello]), None))),
            vec![("hello", Style::Match), (" world", Style::None)]
        );
        // A match past the end of the line marks what it covers.
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(0..20), &layered(&line, Some(&[rld]), None))),
            vec![("hello wo", Style::None), ("rld", Style::Match)]
        );
    }

    #[test]
    fn display_segments_marks_every_match_of_the_line() {
        let line = Line::from("hello world hello");
        assert_eq!(
            segments_as_pairs(&line.display_segments(
                &(0..20),
                &layered(&line, Some(&[0..5, 12..17]), None)
            )),
            vec![
                ("hello", Style::Match),
                (" world ", Style::None),
                ("hello", Style::Match)
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
                &layered(&line, Some(&[0..5, 12..17]), Some(12..17))
            )),
            vec![
                ("hello", Style::Match),
                (" world ", Style::None),
                ("hello", Style::Active)
            ]
        );
        // An active match without other matches marks only itself.
        let other = Line::from("hello world");
        assert_eq!(
            segments_as_pairs(&other.display_segments(&(0..20), &layered(&other, None, Some(6..11)))),
            vec![("hello ", Style::None), ("world", Style::Active)]
        );
    }

    #[test]
    fn display_segments_marks_digits() {
        let line = Line::from("abc 123 def");
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(0..20), &base_style(&line))),
            vec![
                ("abc ", Style::None),
                ("123", Style::Digit),
                (" def", Style::None)
            ]
        );
    }

    #[test]
    fn display_segments_marks_a_search_match_over_digits() {
        let line = Line::from("123 456");
        let digits = 0..3;
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(0..20), &layered(&line, Some(&[digits]), None))),
            vec![
                ("123", Style::Match),
                (" ", Style::None),
                ("456", Style::Digit)
            ]
        );
    }

    #[test]
    fn display_segments_marks_a_comment_over_the_digits_inside_it() {
        // The digits of 42 carry a base digit span under the comment;
        // the comment span is added last, so the whole line renders as
        // one comment segment — no digit segment survives.
        let line = Line::from("// let n = 42;");
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(0..20), &base_style(&line))),
            vec![("// let n = 42;", Style::Comment)]
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
            segments_as_pairs(&line.display_segments(&(3..9), &layered(&line, Some(&[world]), None))),
            vec![
                ("lo ", Style::None),
                ("wor", Style::Match),
                ("…", Style::None)
            ]
        );
        // A match that starts left of the window is clipped at it:
        // it still covers l, o, space and w, which are visible.
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(3..20), &layered(&line, Some(&[head]), None))),
            vec![("lo w", Style::Match), ("orld", Style::None)]
        );
        // A match fully outside the window leaves every segment plain.
        assert_eq!(
            segments_as_pairs(&line.display_segments(&(3..9), &layered(&line, Some(&[outside]), None))),
            vec![("lo wor", Style::None), ("…", Style::None)]
        );
    }

    #[test]
    fn display_segments_of_an_empty_line_is_empty() {
        assert_eq!(
            Line::from("").display_segments(&(0..10), &LineStyle::new()),
            Vec::<(String, Style)>::new()
        );
    }

    #[test]
    fn number_spans_cover_each_integer() {
        let line = Line::from("abc 123 def 42");
        assert_eq!(line.number_spans(), vec![4..7, 12..14]);
    }

    #[test]
    fn number_spans_of_a_line_without_numbers_is_empty() {
        let line = Line::from("abc def");
        assert_eq!(line.number_spans(), Vec::<Range<usize>>::new());
    }

    #[test]
    fn number_spans_of_an_empty_line_is_empty() {
        let line = Line::from("");
        assert_eq!(line.number_spans(), Vec::<Range<usize>>::new());
    }

    #[test]
    fn number_spans_split_on_characters_numbers_cannot_be_built_from() {
        // g and h are no hex digits and no prefix letters, so they
        // break the runs.
        let line = Line::from("1g2h3");
        assert_eq!(line.number_spans(), vec![0..1, 2..3, 4..5]);
    }

    #[test]
    fn number_spans_do_not_count_a_keycap_emoji_as_a_number() {
        // A keycap is a digit plus a combining enclosing keycap: one
        // multi-char grapheme cluster, so it is not a number character.
        let line = Line::from("5\u{20e3}");
        assert_eq!(line.number_spans(), Vec::<Range<usize>>::new());
    }

    #[test]
    fn number_spans_cover_floats_with_one_dot_not_at_an_edge() {
        // 3.14 is a float; .5 starts with the dot, 5. ends with it, and
        // 1.2.3 carries two dots, so none of them is a float.
        let line = Line::from("3.14 .5 5. 1.2.3");
        assert_eq!(line.number_spans(), vec![0..4]);
    }

    #[test]
    fn number_spans_cover_scientific_numbers() {
        // 50e10: an integer, an e, digits. 1.5e3: a float, an e, digits.
        // 5e and e5 miss a side; 5e1.0 and 1.e3 carry a float where an
        // integer is required.
        let line = Line::from("50e10 1.5e3 5e e5 5e1.0 1.e3");
        assert_eq!(line.number_spans(), vec![0..5, 6..11]);
    }

    #[test]
    fn number_spans_cover_underscore_separators_between_characters() {
        // 1_000 is a number with a visual separator; _1_000 starts with
        // one and 1_000_ ends with one, so neither is a number.
        let line = Line::from("1_000 _1_000 1_000_");
        assert_eq!(line.number_spans(), vec![0..5]);
    }

    #[test]
    fn number_spans_cover_hexadecimal_literals() {
        // 0x1F and 0Xdeadbeef are hex literals, in either prefix case.
        // 0x carries no digits, and 123x is neither a decimal integer
        // nor a prefixed literal, so neither is a number.
        let line = Line::from("0x1F 0Xdeadbeef 0x 123x");
        assert_eq!(line.number_spans(), vec![0..4, 5..15]);
    }

    #[test]
    fn number_spans_cover_binary_literals() {
        // 0b101 and 0B1111 are binary literals; 0b2 is not, because 2
        // is not a binary digit.
        let line = Line::from("0b101 0B1111 0b2");
        assert_eq!(line.number_spans(), vec![0..5, 6..12]);
    }

    #[test]
    fn number_spans_cover_octal_literals() {
        // 0o755 and 0O123 are octal literals; 0o8 is not, because 8 is
        // not an octal digit.
        let line = Line::from("0o755 0O123 0o8");
        assert_eq!(line.number_spans(), vec![0..5, 6..11]);
    }

    #[test]
    fn number_spans_of_a_run_glued_to_letters_covers_nothing() {
        // Hex digits and the prefix letters are number characters, so
        // 1f2 is one run: it forms no number, so nothing in it is
        // highlighted — and neither is the 123 of abc123def, which is
        // the run ab123 (c breaks the run) glued to the run def.
        let no_spans = Vec::<Range<usize>>::new();
        assert_eq!(Line::from("1f2").number_spans(), no_spans);
        assert_eq!(Line::from("abc123def").number_spans(), no_spans);
    }

    #[test]
    fn char_literal_end_of_a_plain_char_literal_is_past_the_closing_quote() {
        let line = Line::from("'a'");
        assert_eq!(line.char_literal_end(0), Some(3));
    }

    #[test]
    fn char_literal_end_of_an_empty_literal_covers_the_two_quotes() {
        let line = Line::from("''");
        assert_eq!(line.char_literal_end(0), Some(2));
    }

    #[test]
    fn char_literal_end_of_a_quoted_quote_is_past_the_last_quote() {
        // The backslash escapes the quote it precedes, so the closer is
        // the very last quote.
        let line = Line::from("'\\''");
        assert_eq!(line.char_literal_end(0), Some(4));
    }

    #[test]
    fn starts_lifetime_run_of_a_quote_followed_by_an_unclosed_run_is_true() {
        // The run after the quote is not immediately closed by a quote,
        // so the quote opens a lifetime run.
        let line = Line::from("'static");
        assert!(line.starts_lifetime_run(0));
        let line = Line::from("'_");
        assert!(line.starts_lifetime_run(0));
        let line = Line::from("'a1_b2");
        assert!(line.starts_lifetime_run(0));
        // The run is followed by a space, not a quote: still a lifetime.
        let line = Line::from("'a ");
        assert!(line.starts_lifetime_run(0));
    }

    #[test]
    fn starts_lifetime_run_of_a_quote_followed_by_a_closed_run_is_false() {
        // The run after the quote is immediately closed by a quote: it
        // is the interior of a character literal, not a lifetime.
        let line = Line::from("'a'");
        assert!(!line.starts_lifetime_run(0));
        let line = Line::from("'test'");
        assert!(!line.starts_lifetime_run(0));
    }

    #[test]
    fn starts_lifetime_run_of_a_quote_without_a_run_is_false() {
        let line = Line::from("'");
        assert!(!line.starts_lifetime_run(0));
        let line = Line::from("' ");
        assert!(!line.starts_lifetime_run(0));
        let line = Line::from("'.");
        assert!(!line.starts_lifetime_run(0));
        // é is not an ASCII run character.
        let line = Line::from("'\u{e9}");
        assert!(!line.starts_lifetime_run(0));
    }

    #[test]
    fn char_literal_end_skips_a_quote_that_opens_a_lifetime_run() {
        // The quote of 'static opens a lifetime run, so it is no
        // closer: the literal that opened before it keeps scanning and
        // finds no closer.
        let line = Line::from("'x 'static");
        assert_eq!(line.char_literal_end(0), None);
        // A plain quote still closes: a space after the quote is no run.
        let line = Line::from("'x' 'a");
        assert_eq!(line.char_literal_end(0), Some(3));
        // The run after the closer is immediately closed by a quote, so
        // the closer is a plain quote, not a lifetime.
        let line = Line::from("'x' 'test'");
        assert_eq!(line.char_literal_end(0), Some(3));
    }

    #[test]
    fn char_literal_end_of_a_lone_quote_is_none() {
        let line = Line::from("'");
        assert_eq!(line.char_literal_end(0), None);
    }

    #[test]
    fn char_literal_end_without_a_closing_quote_is_none() {
        let line = Line::from("'a");
        assert_eq!(line.char_literal_end(0), None);
    }

    #[test]
    fn char_literal_end_of_a_triple_quote_covers_the_first_two() {
        let line = Line::from("'''");
        assert_eq!(line.char_literal_end(0), Some(2));
        // The third quote opens no literal: it has no closer.
        assert_eq!(line.char_literal_end(2), None);
    }

    #[test]
    fn char_literal_end_scans_a_multibyte_interior_in_grapheme_space() {
        // 'é' is two bytes in UTF-8 but one grapheme, so the literal
        // ends at grapheme 3, not byte 4.
        let line = Line::from("'é'");
        assert_eq!(line.char_literal_end(0), Some(3));
    }

    #[test]
    fn char_literal_end_of_a_line_ending_in_the_escape_is_none() {
        // The backslash escapes past the end of the line: there is no
        // closer.
        let line = Line::from("'\\");
        assert_eq!(line.char_literal_end(0), None);
    }

    #[test]
    fn char_literal_end_of_a_backslash_literal_covers_the_whole_run() {
        // The backslash directly after the opening quote escapes the
        // grapheme it precedes — the second backslash — so the closer is
        // the last quote.
        let line = Line::from("'\\\\'");
        assert_eq!(line.char_literal_end(0), Some(4));
    }

    #[test]
    fn char_literal_end_at_a_non_quote_grapheme_is_none() {
        let line = Line::from("abc");
        assert_eq!(line.char_literal_end(0), None);
    }

    #[test]
    fn char_literal_end_past_the_end_of_the_line_is_none() {
        let line = Line::from("'a'");
        assert_eq!(line.char_literal_end(3), None);
        assert_eq!(line.char_literal_end(100), None);
    }

    #[test]
    fn lifetime_spans_of_an_empty_line_is_empty() {
        let line = Line::from("");
        assert_eq!(line.lifetime_spans(), Vec::<Range<usize>>::new());
    }

    #[test]
    fn lifetime_spans_cover_the_quote_and_the_whole_run_after_it() {
        // 'static is seven graphemes: the quote plus the maximal run of
        // identifier characters.
        let line = Line::from("'static");
        assert_eq!(line.lifetime_spans(), vec![0..7]);
        // Digits and underscores are run characters: the run is maximal.
        let line = Line::from("'a1_b2");
        assert_eq!(line.lifetime_spans(), vec![0..6]);
        // An underscore alone is a run: ' is two graphemes.
        let line = Line::from("'_");
        assert_eq!(line.lifetime_spans(), vec![0..2]);
    }

    #[test]
    fn lifetime_spans_of_a_quote_without_a_run_is_empty() {
        let no_spans = Vec::<Range<usize>>::new();
        // A quote at the end of the line starts no run.
        assert_eq!(Line::from("'").lifetime_spans(), no_spans);
        // A space follower starts no run.
        assert_eq!(Line::from("' ").lifetime_spans(), no_spans);
        // A dot after the quote is not a run character.
        assert_eq!(Line::from("'.x").lifetime_spans(), no_spans);
    }

    #[test]
    fn lifetime_spans_of_a_non_ascii_follower_is_empty() {
        // é is not an ASCII identifier character, so the run is empty
        // and the quote stays unreported.
        let line = Line::from("'\u{e9}");
        assert_eq!(line.lifetime_spans(), Vec::<Range<usize>>::new());
    }

    #[test]
    fn lifetime_spans_find_each_quote_with_a_run_in_line_order() {
        let line = Line::from("'a 'b");
        assert_eq!(line.lifetime_spans(), vec![0..2, 3..5]);
    }

    #[test]
    fn lifetime_spans_of_a_multibyte_line_works_in_grapheme_space() {
        // é is one grapheme: the quote before it is not a lifetime, and
        // the 'a' after it is, measured in graphemes.
        let line = Line::from("'\u{e9} 'a");
        assert_eq!(line.lifetime_spans(), vec![3..5]);
    }

    #[test]
    fn word_spans_of_an_empty_line_is_empty() {
        let line = Line::from("");
        assert_eq!(line.word_spans(), Vec::<Range<usize>>::new());
    }

    #[test]
    fn word_spans_cover_the_whole_line_when_it_is_one_word() {
        let line = Line::from("abc_def");
        assert_eq!(line.word_spans(), vec![0..7]);
    }

    #[test]
    fn word_spans_join_digits_and_underscores_into_one_run() {
        let line = Line::from("a1_b2");
        assert_eq!(line.word_spans(), vec![0..5]);
    }

    #[test]
    fn word_spans_split_on_punctuation() {
        let line = Line::from("a-b_c");
        assert_eq!(line.word_spans(), vec![0..1, 2..5]);
    }

    #[test]
    fn word_spans_end_at_a_punctuation_boundary() {
        // Punctuation is not a word character, so the run is exactly
        // `true` — the whole-word match the `!` allows.
        let line = Line::from("true!");
        assert_eq!(line.word_spans(), vec![0..4]);
    }

    #[test]
    fn word_spans_do_not_count_a_unicode_grapheme_as_a_word_character() {
        // 'é' is not an ASCII identifier character, so it breaks the
        // runs: "aéb" is the two runs "a" and "b".
        let line = Line::from("aéb");
        assert_eq!(line.word_spans(), vec![0..1, 2..3]);
    }

    #[test]
    fn word_spans_do_not_count_a_tab_as_a_word_character() {
        let line = Line::from("a\tb");
        assert_eq!(line.word_spans(), vec![0..1, 2..3]);
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
