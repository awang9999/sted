use crate::common::line::Line;
use crate::common::types::FileType;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;

/// How a span of a line's text is styled. The view renders each kind
/// differently: plain spans in the terminal's default colors, search
/// matches in the match highlight, the active match — the one the caret
/// sits on — in its own, stronger highlight, numbers in the digit
/// color, keywords in the keyword color, common type names in the type
/// color, literals and enum-variant names in the literal color, and
/// character literals in the character color. Lifetime specifiers are
/// drawn in the lifetime color.
///
/// This is the palette of the buffer's styling: a syntax coloring pass
/// (keywords, strings, comments, ...) adds to it, and the view learns to
/// draw the new kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// No style: drawn with the terminal's default colors.
    None,
    /// A search match that is not the active one.
    Match,
    /// The active search match, where the caret sits.
    Active,
    /// A number — an integer, a float, a scientific number, a
    /// hexadecimal, binary, or octal literal, and the same with
    /// underscore separators: drawn in the digit color.
    Digit,
    /// A keyword of the language being highlighted: drawn in the keyword
    /// color.
    Keyword,
    /// A common type name — an integer, float, or primitive type, or a
    /// frequent standard-library type: drawn in the type color.
    Type,
    /// A literal or enum-variant name — a boolean literal or a variant
    /// of a common standard-library enum: drawn in the literal color.
    Literal,
    /// A character literal: the opening quote, its interior, and the
    /// closing quote: drawn in the character color.
    Char,
    /// A lifetime specifier: the quote that opens a lifetime name and
    /// the name itself — `'static`, `'_`, `'a_b2` —: drawn in the
    /// lifetime color.
    Lifetime,
}

/// The styling of one line of a buffer: the grapheme spans that carry a
/// style, in the order they were added.
///
/// A span is a range of grapheme indices of the line the styling belongs
/// to, so it can be handed to the line as-is when the line builds its
/// annotated string. Spans may overlap, and the last span added wins, so
/// a caller can layer a stronger style (an active match) over a weaker
/// one (a plain match, a digit). A style that spans several lines of the
/// buffer — a multi-line comment, say — is one span in each of the lines
/// it covers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineStyle {
    spans: Vec<(Range<usize>, Style)>,
}

impl LineStyle {
    /// The styling of a line without any styled spans.
    pub fn new() -> Self {
        Self::default()
    }

    /// Styles the grapheme span `span` with `style`. A span that
    /// overlaps one added earlier takes precedence over it.
    pub fn add(&mut self, span: Range<usize>, style: Style) {
        if !span.is_empty() {
            self.spans.push((span, style));
        }
    }

    /// The style of the grapheme at `index`: the style of the last span
    /// added that covers it, or [`Style::None`] when no span covers it.
    pub fn at(&self, index: usize) -> Style {
        self.spans
            .iter()
            .rev()
            .find(|(span, _)| span.contains(&index))
            .map_or(Style::None, |(_, style)| *style)
    }

    /// The styled spans in the order they were added.
    // Only the test suite reads the spans directly so far; the view
    // resolves them grapheme by grapheme through [`LineStyle::at`].
    #[allow(dead_code)]
    pub fn spans(&self) -> &[(Range<usize>, Style)] {
        &self.spans
    }
}

/// A syntax highlighter for one language: the rules that decide how a
/// line's text is styled at the base level, before the buffer's other
/// styles (search matches, the active match) are layered on top.
///
/// The [`Styles`] of a buffer delegates its base styling to the
/// highlighter of the buffer's file type, so a new language is a new
/// implementation of this trait: `Styles` and the buffer learn nothing
/// about it. Each language ships one highlighter, held behind an
/// [`Arc`] so a `Styles` — and with it a buffer — stays `Clone`.
pub trait SyntaxHighlighter: Send + Sync {
    /// The type of file this highlighter styles: the file type it is
    /// registered as the highlighter of.
    fn file_type(&self) -> FileType;

    /// The base styling of `line`'s text under this language's rules.
    fn base_style(&self, line: &Line) -> LineStyle;
}

/// A syntax highlighter that styles no text at all: the base styling of
/// every line is empty. The rules of plain text.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlainTextSyntaxHighlighter;

impl PlainTextSyntaxHighlighter {
    /// The highlighter's file type: [`FileType::PlainText`].
    pub const FILE_TYPE: FileType = FileType::PlainText;
}

impl SyntaxHighlighter for PlainTextSyntaxHighlighter {
    fn file_type(&self) -> FileType {
        Self::FILE_TYPE
    }

    /// Plain text carries no base styling: the digits stay unstyled, so
    /// only the other styles — a search match, say — ever appear on it.
    fn base_style(&self, _line: &Line) -> LineStyle {
        LineStyle::new()
    }
}

/// The syntax highlighter of a Rust source file: styles each number of
/// a line with [`Style::Digit`]: an integer (a run of ASCII digits), a
/// float (exactly one dot, not at the start or the end), a scientific
/// number (an integer or a float, a lowercase `e`, then digits), a
/// hexadecimal literal (`0x` or `0X`, then hex digits), a binary
/// literal (`0b` or `0B`, then 0s and 1s), an octal literal (`0o` or
/// `0O`, then 0-7), and each of those with underscores as visual
/// separators — a separator never at the start or the end of the
/// number. Whole words of the line are styled too: each word is one
/// maximal run of ASCII identifier characters, and a word that exactly
/// equals one of the reference keywords (minus `static`), one of the
/// common type names, or one of the literal and enum-variant names
/// carries that group's style — a word in no list is left plain. The
/// word spans are added after the number spans, so a type like `u128`
/// or `i8` wins the overlap of its inner digit spans and renders fully
/// in the type color.
///
/// Character literals are styled too: everything from an opening quote
/// to its closing quote — the opening quote, the interior, and the
/// closing quote — carries [`Style::Char`], whatever the interior is.
/// A backslash directly after the opening quote escapes the grapheme it
/// precedes, so it is not the closing quote (`'\''` is one literal);
/// at most one escape is honored, and only in that position. The quotes
/// pair naively, left to right, so an apostrophe in prose pairs with
/// the next quote on the line. A quote with no closer styles nothing.
///
/// Lifetime specifiers are styled too: a quote that opens no character
/// literal and is directly followed by at least one ASCII identifier
/// character — a letter, a digit, or an underscore — is one, and the
/// span covers the quote plus the maximal run of such characters that
/// follows it: `'static`, `'_`, and `'a_b2` all carry
/// [`Style::Lifetime`]. A quote at the end of a line, or one followed
/// by a space, punctuation, or a non-ASCII character, is no lifetime.
/// The lifetime spans are added after the character-literal spans on
/// purpose: a character literal like `'a'` claims its closing quote, so
/// the `'a` inside it is not read as a lifetime, and a quote the
/// character scan left unpaired — an unclosed `'a` — is still a
/// lifetime. As with the character literals, the pairing is naive: an
/// apostrophe in prose that opens no literal starts a lifetime run of
/// whatever identifier characters follow it on the line.
#[derive(Clone, Copy, Debug, Default)]
pub struct RustSyntaxHighlighter;

impl RustSyntaxHighlighter {
    /// The highlighter's file type: [`FileType::Rust`].
    pub const FILE_TYPE: FileType = FileType::Rust;
}

/// The keywords of the Rust reference: the reference's keyword list,
/// minus `static` — it is used so often as a lifetime prefix that
/// highlighting it is noise. `true` and `false` are not keywords here;
/// they are literals in [`LITERALS`]. The reserved-but-obsolete keywords
/// (`abstract`, `box`, `become`, ...) are not highlighted.
const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn",
    "else", "enum", "extern", "fn", "for", "if", "impl", "in", "let",
    "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self",
    "Self", "struct", "super", "trait", "type", "unsafe", "use", "where",
    "while",
];

/// The common type names: the integer and float primitives, `bool` and
/// `char`, and the standard-library types Rust code reaches for most.
const TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize",
    "u8", "u16", "u32", "u64", "u128", "usize",
    "f32", "f64",
    "bool", "char",
    "Option", "Result",
    "String", "str",
    "Vec", "HashMap",
];

/// The literal and enum-variant names: the boolean literals and the
/// variants of the most common standard-library enums.
const LITERALS: &[&str] = &["Some", "None", "Ok", "Err", "true", "false"];

/// The position the scan resumes from after walking the plain graphemes
/// from `position` up to (exclusive) `limit`: a quote that opens a
/// character literal claims everything up to the literal's end — the
/// grapheme just past its closing quote — and the remaining graphemes
/// stay plain.
fn advance_past_literals(
    position: usize,
    limit: usize,
    line: &Line,
    style: &mut LineStyle,
    char_spans: &mut Vec<Range<usize>>,
) -> usize {
    let mut position = position;
    while position < limit {
        if let Some(end) = line.char_literal_end(position) {
            style.add(position..end, Style::Char);
            char_spans.push(position..end);
            position = end;
            continue;
        }
        position = position.saturating_add(1);
    }
    position
}

/// The style a whole word carries under the Rust rules: the style of
/// the list the word is in, or `None` when it is in no list.
fn style_of_word(word: &str) -> Option<Style> {
    if KEYWORDS.contains(&word) {
        Some(Style::Keyword)
    } else if TYPES.contains(&word) {
        Some(Style::Type)
    } else if LITERALS.contains(&word) {
        Some(Style::Literal)
    } else {
        None
    }
}

impl SyntaxHighlighter for RustSyntaxHighlighter {
    fn file_type(&self) -> FileType {
        Self::FILE_TYPE
    }

    /// A digit style over each of the line's numbers, the style of each
    /// of its words that is a keyword, a common type name, or a literal
    /// or enum-variant name, the character style over each character
    /// literal — the opening quote, its interior, and its closing
    /// quote — and the lifetime style over each lifetime specifier —
    /// a quote that opens no character literal, followed by a run of
    /// identifier characters.
    ///
    /// The spans land in two phases, and a later span wins an overlap:
    /// first the digit spans, then the word spans — so a type like
    /// `u128` or `i8` renders fully in the type color, over the digit
    /// spans of its inner digits. The words and the character literals
    /// are found in one scan, in line order: a word run that is in one
    /// of the lists carries that list's style, and a quote that opens a
    /// character literal hands the scan to the literal parser, whose
    /// end — the grapheme just past the closing quote — is where the
    /// scan resumes. The interior words of a literal are never visited,
    /// so no word or keyword span is ever added inside one, and a
    /// literal's span wins the overlap of any digit spans inside it.
    fn base_style(&self, line: &Line) -> LineStyle {
        let mut style = LineStyle::new();
        for span in line.number_spans() {
            style.add(span, Style::Digit);
        }
        // The word and character-literal spans are added after the
        // number spans on purpose: a later span wins an overlap, so a
        // type like `u128` or `i8` renders fully in the type color,
        // over the digit spans of its inner digits.
        let words = line.word_spans();
        let mut position = 0;
        let mut char_spans: Vec<Range<usize>> = Vec::new();
        for span in &words {
            position = advance_past_literals(position, span.start, line, &mut style, &mut char_spans);
            if position > span.start {
                // The word is inside a character literal: its opening
                // quote claimed it, so it carries no style of its own.
                continue;
            }
            let word = line.run_text(span.start, span.end);
            if let Some(word_style) = style_of_word(word) {
                style.add(span.clone(), word_style);
            }
            position = span.end;
        }
        // A quote may open a literal in the plain tail after the last
        // word too.
        advance_past_literals(position, line.grapheme_count(), line, &mut style, &mut char_spans);
        // The lifetime spans are added after all the character spans,
        // and in line order, on purpose: a quote the character scan
        // left unpaired — an unclosed `'a` — still starts a lifetime
        // run, and a lifetime span that falls inside a character
        // literal is dropped instead: the literal's closing quote is
        // not the quote a lifetime opens, so a literal like `'a'` stays
        // fully a character, with no lifetime span added for the `'a`
        // inside it. Where a lifetime span does overlap another span,
        // the later span wins the overlap, as it does for the word
        // styles.
        for span in line.lifetime_spans() {
            if !char_spans.iter().any(|c| c.contains(&span.start)) {
                style.add(span, Style::Lifetime);
            }
        }
        style
    }
}

/// The highlighter that styles a file of `file_type`: the language's
/// highlighter, or the plain text one — which styles nothing — when the
/// file type carries no highlighting rules of its own.
fn highlighter_for(file_type: FileType) -> Arc<dyn SyntaxHighlighter> {
    match file_type {
        FileType::PlainText => Arc::new(PlainTextSyntaxHighlighter),
        FileType::Rust => Arc::new(RustSyntaxHighlighter),
    }
}

/// The styling of the lines of a buffer: one [`LineStyle`] per line,
/// kept in parallel with the buffer's lines.
///
/// This structure is the single owner of how the buffer's text is
/// styled: the base styling of the text for now, and more coloring as it
/// is added. It stores, for each line, the styling information, and the
/// view hands a line its styling when the line builds its annotated
/// string.
///
/// The base styling is delegated to the [`SyntaxHighlighter`] of the
/// buffer's file type: the highlighter decides which rules apply. For
/// now, digits and words are styled only in Rust files, while plain
/// text carries no base styling at all.
#[derive(Clone)]
pub struct Styles {
    lines: Vec<LineStyle>,
    /// The highlighter whose rules the base styling of the lines is
    /// built under: the one of the buffer's file type. Held behind an
    /// [`Arc`] so a `Styles` — and with it a buffer — stays `Clone`.
    highlighter: Arc<dyn SyntaxHighlighter>,
}

impl Default for Styles {
    /// The styling of a new, unnamed document: no lines, and the plain
    /// text highlighter — which styles nothing — as the rules.
    fn default() -> Self {
        Self::for_lines(FileType::PlainText, &[])
    }
}

impl fmt::Debug for Styles {
    /// The lines and the type of the highlighter in use — the
    /// highlighter itself is a black box to the formatter.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Styles")
            .field("lines", &self.lines)
            .field("file_type", &self.highlighter.file_type())
            .finish()
    }
}

impl Styles {
    /// The styling for a slice of the lines of a `file_type` file: the
    /// base styling of each line under the file type's highlighter's
    /// rules, with no other styles.
    pub fn for_lines(file_type: FileType, lines: &[Line]) -> Self {
        let highlighter = highlighter_for(file_type);
        Self {
            lines: lines
                .iter()
                .map(|line| highlighter.base_style(line))
                .collect(),
            highlighter,
        }
    }

    /// The styling of the line at `row`, or `None` when the row is out
    /// of range.
    pub fn line(&self, row: usize) -> Option<&LineStyle> {
        self.lines.get(row)
    }

    /// Inserts a line's base styling at `row`, shifting the following
    /// lines' styling down: the mirror of a line being inserted into the
    /// buffer.
    pub fn insert_line(&mut self, row: usize, line: &Line) {
        self.lines
            .insert(row.min(self.lines.len()), self.highlighter.base_style(line));
    }

    /// Removes the styling of the line at `row`, shifting the following
    /// lines' styling up: the mirror of a line being removed from the
    /// buffer. Rows out of range are ignored.
    pub fn remove_line(&mut self, row: usize) {
        if row < self.lines.len() {
            self.lines.remove(row);
        }
    }

    /// Rebuilds the styling of the line at `row` from the line's current
    /// text, dropping any styles previously added to it: the mirror of
    /// the line's text being edited. Rows out of range are ignored.
    pub fn refresh_line(&mut self, row: usize, line: &Line) {
        if let Some(style) = self.lines.get_mut(row) {
            *style = self.highlighter.base_style(line);
        }
    }

    /// Updates the type the styled lines belong to. When it changes, the
    /// highlighter's rules change with it, so the base styling of every
    /// line in `lines` is rebuilt under the new rules: a re-typed buffer
    /// (a save-as to a new extension) gets its digits highlighted or
    /// un-highlighted accordingly.
    pub fn set_file_type(&mut self, file_type: FileType, lines: &[Line]) {
        if self.highlighter.file_type() == file_type {
            return;
        }
        self.highlighter = highlighter_for(file_type);
        self.lines = lines
            .iter()
            .map(|line| self.highlighter.base_style(line))
            .collect();
    }

    /// The type of file the styled lines belong to: the type of the
    /// highlighter whose rules the base styling is built under.
    // Only the test suite reads it so far; the buffer keeps its own
    // copy of the type.
    #[allow(dead_code)]
    pub fn file_type(&self) -> FileType {
        self.highlighter.file_type()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The base styling of `line` of a Rust file: where the digits are
    /// styled.
    fn style_of(line: &Line) -> LineStyle {
        Styles::for_lines(FileType::Rust, std::slice::from_ref(line))
            .line(0)
            .expect("the slice has the line")
            .clone()
    }

    #[test]
    fn at_of_a_style_without_spans_is_none() {
        let style = LineStyle::new();
        assert_eq!(style.at(0), Style::None);
        assert_eq!(style.at(100), Style::None);
    }

    #[test]
    fn at_reports_the_style_of_the_span_covering_the_grapheme() {
        let mut style = LineStyle::new();
        style.add(2..5, Style::Match);
        assert_eq!(style.at(1), Style::None);
        assert_eq!(style.at(2), Style::Match);
        assert_eq!(style.at(4), Style::Match);
        assert_eq!(style.at(5), Style::None);
    }

    #[test]
    fn a_later_span_wins_over_an_earlier_span_it_overlaps() {
        let mut style = LineStyle::new();
        style.add(0..5, Style::Match);
        style.add(2..7, Style::Active);
        assert_eq!(style.at(0), Style::Match);
        assert_eq!(style.at(2), Style::Active);
        assert_eq!(style.at(6), Style::Active);
    }

    #[test]
    fn add_ignores_an_empty_span() {
        let mut style = LineStyle::new();
        style.add(3..3, Style::Match);
        assert_eq!(style.spans().len(), 0);
        assert_eq!(style.at(3), Style::None);
    }

    #[test]
    fn spans_reports_the_spans_in_the_order_they_were_added() {
        let mut style = LineStyle::new();
        style.add(0..2, Style::Digit);
        style.add(4..5, Style::Match);
        assert_eq!(
            style.spans(),
            &[(0..2, Style::Digit), (4..5, Style::Match)]
        );
    }

    #[test]
    fn the_base_styling_of_a_line_marks_its_digits() {
        let line = Line::from("abc 123 def 42");
        assert_eq!(
            style_of(&line).spans(),
            &[(4..7, Style::Digit), (12..14, Style::Digit)]
        );
    }

    #[test]
    fn the_base_styling_of_a_line_without_digits_has_no_spans() {
        let line = Line::from("abc def");
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(style_of(&line).spans(), no_spans);
    }

    #[test]
    fn for_lines_of_plain_text_does_not_style_the_digits() {
        let line = Line::from("abc 123 def 42");
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(
            Styles::for_lines(FileType::PlainText, std::slice::from_ref(&line))
                .line(0)
                .unwrap()
                .spans(),
            no_spans
        );
    }

    #[test]
    fn editing_a_plain_text_line_keeps_its_digits_unstyled() {
        let mut line = Line::from("abc 123");
        let mut styles = Styles::for_lines(FileType::PlainText, &[line.clone()]);
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(styles.line(0).unwrap().spans(), no_spans);

        // Editing the line rebuilds its styling under the plain text
        // rules: the digits stay unstyled.
        line.insert_char(0, '5');
        styles.refresh_line(0, &line);
        assert_eq!(styles.line(0).unwrap().spans(), no_spans);

        // A newly inserted line is styled with the same rules.
        styles.insert_line(0, &Line::from("42"));
        assert_eq!(styles.line(0).unwrap().spans(), no_spans);
        assert_eq!(styles.line(1).unwrap().spans(), no_spans);
    }

    #[test]
    fn set_file_type_switches_the_styling_with_the_type() {
        let lines = [Line::from("abc 123")];
        let mut styles = Styles::for_lines(FileType::Rust, &lines);
        assert_eq!(styles.line(0).unwrap().spans(), &[(4..7, Style::Digit)]);

        // Re-typing the buffer to plain text removes the digit styling.
        styles.set_file_type(FileType::PlainText, &lines);
        assert_eq!(styles.file_type(), FileType::PlainText);
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(styles.line(0).unwrap().spans(), no_spans);

        // ... and re-typing it to Rust restores it.
        styles.set_file_type(FileType::Rust, &lines);
        assert_eq!(styles.file_type(), FileType::Rust);
        assert_eq!(styles.line(0).unwrap().spans(), &[(4..7, Style::Digit)]);
    }

    #[test]
    fn for_lines_holds_one_style_per_line() {
        let lines = [Line::from("1"), Line::from("2"), Line::from("c")];
        let styles = Styles::for_lines(FileType::Rust, &lines);
        assert_eq!(styles.line(0).unwrap().spans(), &[(0..1, Style::Digit)]);
        assert_eq!(styles.line(1).unwrap().spans(), &[(0..1, Style::Digit)]);
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(styles.line(2).unwrap().spans(), no_spans);
        assert_eq!(styles.line(3), None);
    }

    #[test]
    fn insert_line_places_the_base_styling_and_shifts_the_rest_down() {
        let mut styles = Styles::for_lines(FileType::Rust, &[Line::from("1")]);
        let line = Line::from("2");
        styles.insert_line(0, &line);
        assert_eq!(styles.line(0).unwrap().spans(), &[(0..1, Style::Digit)]);
        assert_eq!(styles.line(1).unwrap().spans(), &[(0..1, Style::Digit)]);
        assert_eq!(styles.line(2), None);
    }

    #[test]
    fn remove_line_drops_the_styling_and_shifts_the_rest_up() {
        let mut styles =
            Styles::for_lines(FileType::Rust, &[Line::from("1"), Line::from("2")]);
        styles.remove_line(0);
        assert_eq!(styles.line(0).unwrap().spans(), &[(0..1, Style::Digit)]);
        assert_eq!(styles.line(1), None);
        // Removing out of range is a no-op.
        styles.remove_line(5);
        assert_eq!(styles.line(0).unwrap().spans(), &[(0..1, Style::Digit)]);
    }

    #[test]
    fn refresh_line_rebuilds_the_styling_from_the_line_text() {
        let mut line = Line::from("abc 123");
        let mut styles = Styles::for_lines(FileType::Rust, &[line.clone()]);
        assert_eq!(
            styles.line(0).unwrap().spans(),
            &[(4..7, Style::Digit)]
        );
        // Editing the line's text rebuilds its styling, so the spans
        // follow the digits to their new graphemes. The z keeps the 5
        // from joining the digits of 123 in one run.
        line.insert_char(0, 'z');
        styles.refresh_line(0, &line);
        assert_eq!(
            styles.line(0).unwrap().spans(),
            &[(5..8, Style::Digit)]
        );
        line.insert_char(0, '5');
        styles.refresh_line(0, &line);
        assert_eq!(
            styles.line(0).unwrap().spans(),
            &[(0..1, Style::Digit), (6..9, Style::Digit)]
        );
        // Refreshing a row out of range is a no-op.
        styles.refresh_line(7, &line);
        assert_eq!(
            styles.line(0).unwrap().spans(),
            &[(0..1, Style::Digit), (6..9, Style::Digit)]
        );
    }

    #[test]
    fn the_rust_highlighter_styles_each_number() {
        let highlighter = RustSyntaxHighlighter;
        assert_eq!(highlighter.file_type(), FileType::Rust);
        // 42 is an integer, and 0x1f is one hexadecimal literal, not
        // the integers 0 and 1; let is a keyword.
        let line = Line::from("let x = 42; // 0x1f");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(8..10, Style::Digit), (15..19, Style::Digit), (0..3, Style::Keyword)]
        );
    }

    #[test]
    fn the_rust_highlighter_styles_hexadecimal_literals() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("a = 0xFF b = 0xdead");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(4..8, Style::Digit), (13..19, Style::Digit)]
        );
        // A bare prefix and a non-hex digit end the literal: 0x is not
        // a number, and in 0x1G the run 0x1 is one.
        let line = Line::from("c = 0x d = 0x1G");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(11..14, Style::Digit)]
        );
    }

    #[test]
    fn the_rust_highlighter_styles_binary_literals() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("a = 0b1010");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(4..10, Style::Digit)]
        );
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(
            highlighter.base_style(&Line::from("0b2")).spans(),
            no_spans
        );
    }

    #[test]
    fn the_rust_highlighter_styles_octal_literals() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("a = 0o755");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(4..9, Style::Digit)]
        );
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(
            highlighter.base_style(&Line::from("0o8")).spans(),
            no_spans
        );
    }

    #[test]
    fn the_rust_highlighter_styles_floats_with_one_interior_dot() {
        let highlighter = RustSyntaxHighlighter;
        // A dot between digits makes a float: the whole number is
        // styled, the dot included.
        let line = Line::from("pi = 3.14");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(5..9, Style::Digit)]
        );
        // A dot at the start or the end of the run is not a float,
        // and neither is a run with two dots.
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(
            highlighter.base_style(&Line::from(".5 and 5.")).spans(),
            no_spans
        );
        assert_eq!(
            highlighter.base_style(&Line::from("1.2.3")).spans(),
            no_spans
        );
    }

    #[test]
    fn the_rust_highlighter_styles_scientific_numbers() {
        let highlighter = RustSyntaxHighlighter;
        // An integer or a float, an e, then a run of digits: 50e10 and
        // 1.5e3 are both numbers.
        let line = Line::from("a = 50e10 b = 1.5e3");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(4..9, Style::Digit), (14..19, Style::Digit)]
        );
        // A float where an integer is required (5e1.0, 1.e3), a missing
        // side (5e, e5), or a second e (50e10e5) is not a number.
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(
            highlighter
                .base_style(&Line::from("5e1.0 5e e5 50e10e5 1.e3"))
                .spans(),
            no_spans
        );
    }

    #[test]
    fn the_rust_highlighter_styles_underscore_separators_between_characters() {
        let highlighter = RustSyntaxHighlighter;
        // An underscore between characters is a visual separator: the
        // whole number is styled, the separators included.
        let line = Line::from("n = 1_000_000");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(4..13, Style::Digit)]
        );
        // A separator at the start or the end of the word is not a
        // separator: neither run is a number.
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(
            highlighter.base_style(&Line::from("_1_000 1_000_")).spans(),
            no_spans
        );
    }

    /// The raw content of a fixture file under `testdata/`.
    fn fixture_content(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/testdata/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("fixture should be readable")
    }

    #[test]
    fn the_numbers_fixture_is_highlighted_line_by_line() {
        // The corpus under testdata/numbers.txt pins the highlighter's
        // rules against a realistic line per case: the file is plain
        // text, so the test drives the highlighter directly rather than
        // going through the file type.
        let lines: Vec<Line> =
            fixture_content("numbers.txt").lines().map(Line::from).collect();
        let highlighter = RustSyntaxHighlighter;
        let expected: &[&[(Range<usize>, Style)]] = &[
            &[(0..2, Style::Digit)],
            &[(0..1, Style::Digit), (2..5, Style::Digit)],
            &[(0..4, Style::Digit), (5..8, Style::Digit)],
            &[(0..7, Style::Digit)],
            &[(0..5, Style::Digit)],
            &[(0..5, Style::Digit)],
            &[(0..7, Style::Digit)],
            &[(0..5, Style::Digit)],
            &[(0..9, Style::Digit)],
            &[(0..9, Style::Digit)],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[(0..4, Style::Digit)],
            &[(0..6, Style::Digit)],
            &[(0..4, Style::Digit)],
            &[(0..10, Style::Digit)],
            &[(0..5, Style::Digit)],
            &[(0..6, Style::Digit)],
            &[(0..5, Style::Digit)],
            &[(0..5, Style::Digit)],
            &[],
            &[(0..5, Style::Digit)],
            &[],
        ];
        assert_eq!(lines.len(), expected.len(), "the fixture's line count changed");
        for (line, want) in lines.iter().zip(expected.iter().copied()) {
            assert_eq!(
                highlighter.base_style(line).spans(),
                want,
                "line: {line}"
            );
        }
    }

    #[test]
    fn the_rust_words_fixture_is_highlighted_line_by_line() {
        // The corpus under testdata/rust_words.txt pins the word
        // styling against the required behavioral examples and a few
        // realistic Rust lines: the file is plain text, so the test
        // drives the highlighter directly rather than going through the
        // file type.
        let lines: Vec<Line> =
            fixture_content("rust_words.txt").lines().map(Line::from).collect();
        let highlighter = RustSyntaxHighlighter;
        let expected: &[&[(Range<usize>, Style)]] = &[
            &[(0..2, Style::Keyword), (9..13, Style::Keyword)],
            &[
                (8..11, Style::Digit),
                (14..15, Style::Digit),
                (0..3, Style::Keyword),
                (7..11, Style::Type),
            ],
            &[
                (48..49, Style::Digit),
                (0..5, Style::Keyword),
                (10..14, Style::Literal),
                (24..28, Style::Literal),
                (32..37, Style::Literal),
                (44..47, Style::Literal),
            ],
            &[],
            &[(0..4, Style::Literal)],
            &[],
            &[(0..2, Style::Keyword), (12..15, Style::Keyword), (18..20, Style::Keyword)],
            &[
                (30..32, Style::Digit),
                (0..3, Style::Keyword),
                (9..16, Style::Type),
                (17..23, Style::Type),
                (25..28, Style::Type),
                (29..32, Style::Type),
                (37..44, Style::Type),
            ],
            &[(34..35, Style::Digit), (0..5, Style::Keyword), (12..16, Style::Literal), (26..30, Style::Literal)],
            &[
                (18..19, Style::Digit),
                (29..30, Style::Digit),
                (0..3, Style::Keyword),
                (8..10, Style::Keyword),
                (11..15, Style::Literal),
                (22..26, Style::Keyword),
            ],
            // In f64 the whole word is one number run — f is a hex
            // digit — that forms no number, so no digit span is added
            // under the type span.
            &[(0..3, Style::Keyword), (4..10, Style::Keyword), (22..25, Style::Type), (30..33, Style::Type)],
            &[(0..3, Style::Keyword), (7..13, Style::Type), (18..22, Style::Type), (26..28, Style::Literal)],
            &[(32..34, Style::Digit), (0..3, Style::Keyword), (11..13, Style::Keyword), (15..18, Style::Keyword)],
            &[(15..16, Style::Digit), (0..3, Style::Keyword), (7..13, Style::Type), (14..16, Style::Type), (20..24, Style::Literal)],
        ];
        assert_eq!(lines.len(), expected.len(), "the fixture's line count changed");
        for (line, want) in lines.iter().zip(expected.iter().copied()) {
            assert_eq!(
                highlighter.base_style(line).spans(),
                want,
                "line: {line}"
            );
        }
    }

    #[test]
    fn the_rust_chars_fixture_is_highlighted_line_by_line() {
        // The corpus under testdata/rust_chars.txt pins the
        // character-literal styling: every pinned edge case, plus
        // realistic Rust lines. The file is plain text, so the test
        // drives the highlighter directly rather than going through the
        // file type.
        let lines: Vec<Line> =
            fixture_content("rust_chars.txt").lines().map(Line::from).collect();
        let highlighter = RustSyntaxHighlighter;
        let expected: &[&[(Range<usize>, Style)]] = &[
            &[(0..3, Style::Char)],
            &[(0..2, Style::Char)],
            &[(0..4, Style::Char)],
            &[],
            // Unclosed quote: the character scan finds no closer, so
            // the quote and the run after it are a lifetime.
            &[(0..2, Style::Lifetime)],
            &[(0..2, Style::Char)],
            &[(0..3, Style::Char)],
            // Documented naive pairing: the apostrophe of `it's` pairs
            // with the opening quote of `'test'`.
            &[(2..8, Style::Char)],
            &[(0..4, Style::Char)],
            &[(0..3, Style::Char), (6..9, Style::Char)],
            &[],
            &[(0..4, Style::Char)],
            &[(0..20, Style::Char)],
            &[(0..3, Style::Keyword), (7..11, Style::Type), (14..17, Style::Char)],
            &[
                (0..2, Style::Keyword),
                (17..23, Style::Type),
                (24..28, Style::Type),
                (32..36, Style::Literal),
                (37..40, Style::Char),
            ],
            &[
                (17..18, Style::Digit),
                (25..26, Style::Digit),
                (0..5, Style::Keyword),
                (10..13, Style::Char),
            ],
            // The string literal's apostrophe is the line's only quote:
            // the double quotes are no quotes to the character-literal
            // rule, so it pairs with nothing; the run after it is a
            // lifetime.
            &[(0..3, Style::Keyword), (11..13, Style::Lifetime)],
        ];
        assert_eq!(lines.len(), expected.len(), "the fixture's line count changed");
        for (line, want) in lines.iter().zip(expected.iter().copied()) {
            assert_eq!(
                highlighter.base_style(line).spans(),
                want,
                "line: {line}"
            );
        }
    }

    #[test]
    fn the_rust_lifetimes_fixture_is_highlighted_line_by_line() {
        // The corpus under testdata/rust_lifetimes.txt pins the
        // lifetime-specifier styling: every required edge case, plus
        // realistic Rust lines. The file is plain text, so the test
        // drives the highlighter directly rather than going through the
        // file type.
        let lines: Vec<Line> =
            fixture_content("rust_lifetimes.txt").lines().map(Line::from).collect();
        let highlighter = RustSyntaxHighlighter;
        let expected: &[&[(Range<usize>, Style)]] = &[
            &[(0..7, Style::Lifetime)],
            &[(0..2, Style::Lifetime)],
            &[(0..6, Style::Lifetime)],
            &[],
            &[],
            &[],
            &[(0..3, Style::Char)],
            &[(0..4, Style::Char)],
            // Unclosed literal: the character scan finds no closer, so
            // the quote and the run after it are a lifetime.
            &[(0..2, Style::Lifetime)],
            // The whole thing is one character literal: the interior
            // `static` is never read as a lifetime.
            &[(0..8, Style::Char)],
            // `f<'a>`: the `<'a>` is one character literal under the
            // naive pairing, so no lifetime span lands inside it; the
            // `&'static` is a lifetime, added after the word spans.
            &[
                (0..2, Style::Keyword),
                (17..26, Style::Char),
                (28..31, Style::Type),
                (45..48, Style::Type),
                (37..44, Style::Lifetime),
            ],
            &[
                (0..3, Style::Keyword),
                (16..19, Style::Type),
                (8..15, Style::Lifetime),
            ],
            // `f<'a>` is one character literal under the naive
            // pairing: the apostrophe of `f<'a>` pairs with the quote
            // of the first `&'a`, so the literal runs over both; the
            // second `&'a` is a lifetime, added after the word spans.
            &[
                (0..2, Style::Keyword),
                (5..14, Style::Char),
                (16..19, Style::Type),
                (28..31, Style::Type),
                (25..27, Style::Lifetime),
            ],
            // `g<'a, 'b>`: the first quote pairs with the second, so the
            // literal runs over both names; the third quote pairs with
            // the quote of the first `&'a`, and the rest is plain text.
            &[
                (0..2, Style::Keyword),
                (5..10, Style::Char),
                (17..30, Style::Char),
                (32..35, Style::Type),
            ],
            // The apostrophe of `it's` opens no literal, so the run
            // after it is a lifetime: the quote plus the single letter
            // `s`, the naive-pairing consequence.
            &[(2..4, Style::Lifetime)],
            // The apostrophe is not at the start of a word and is
            // followed by a run: it is a lifetime specifier too, the
            // maximal run included.
            &[(3..7, Style::Lifetime)],
            // The apostrophe opens no literal, and the run after it is
            // a keyword: the lifetime span is added after the keyword
            // span and wins the overlap of it.
            &[
                (1..5, Style::Keyword),
                (0..5, Style::Lifetime),
            ],
        ];
        assert_eq!(lines.len(), expected.len(), "the fixture's line count changed");
        for (line, want) in lines.iter().zip(expected.iter().copied()) {
            assert_eq!(
                highlighter.base_style(line).spans(),
                want,
                "line: {line}"
            );
        }
    }

    #[test]
    fn the_rust_highlighter_styles_a_lifetime_specifier() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("'static");
        let style = highlighter.base_style(&line);
        assert_eq!(style.spans(), &[(0..7, Style::Lifetime)]);
        for i in 0..7 {
            assert_eq!(style.at(i), Style::Lifetime, "grapheme {i} of 'static");
        }
    }

    #[test]
    fn the_rust_highlighter_styles_an_elided_lifetime() {
        let highlighter = RustSyntaxHighlighter;
        assert_eq!(
            highlighter.base_style(&Line::from("'_")).spans(),
            &[(0..2, Style::Lifetime)]
        );
    }

    #[test]
    fn the_rust_highlighter_styles_a_lifetime_run_with_digits_and_underscores() {
        let highlighter = RustSyntaxHighlighter;
        // The run after the quote is maximal: digits and underscores
        // are lifetime-name characters, and the whole run is styled.
        let line = Line::from("'a1_b2");
        let style = highlighter.base_style(&line);
        assert_eq!(style.spans(), &[(0..6, Style::Lifetime)]);
        for i in 0..6 {
            assert_eq!(style.at(i), Style::Lifetime, "grapheme {i} of 'a1_b2");
        }
    }

    #[test]
    fn the_rust_highlighter_does_not_style_a_quote_without_a_run() {
        let highlighter = RustSyntaxHighlighter;
        let no_spans: &[(Range<usize>, Style)] = &[];
        // A quote at the end of the line starts no run.
        assert_eq!(
            highlighter.base_style(&Line::from("'")).spans(),
            no_spans
        );
        // A non-ASCII follower starts no run: the class is ASCII-only.
        assert_eq!(
            highlighter.base_style(&Line::from("'\u{e9}")).spans(),
            no_spans
        );
        // A space follower starts no run.
        assert_eq!(
            highlighter.base_style(&Line::from("' ")).spans(),
            no_spans
        );
        // A dot after the quote is not a run character.
        assert_eq!(
            highlighter.base_style(&Line::from("'.x")).spans(),
            no_spans
        );
    }

    #[test]
    fn the_rust_highlighter_keeps_character_literals_characters() {
        let highlighter = RustSyntaxHighlighter;
        // A closed literal is a character, not a lifetime: the closing
        // quote is claimed by the character scan, so the `'a` inside it
        // is never read as a lifetime.
        assert_eq!(
            highlighter.base_style(&Line::from("'a'")).spans(),
            &[(0..3, Style::Char)]
        );
        // The escaped quote is inside the literal, so no lifetime span
        // is added within it either.
        assert_eq!(
            highlighter.base_style(&Line::from("'\\''")).spans(),
            &[(0..4, Style::Char)]
        );
        // `static` inside a closed literal is not a lifetime: the whole
        // thing is one character.
        assert_eq!(
            highlighter.base_style(&Line::from("'static'")).spans(),
            &[(0..8, Style::Char)]
        );
    }

    #[test]
    fn the_rust_highlighter_styles_an_unclosed_quote_as_a_lifetime() {
        let highlighter = RustSyntaxHighlighter;
        // No closing quote: the character scan consumes nothing, so the
        // quote and its run are a lifetime.
        let line = Line::from("'a");
        let style = highlighter.base_style(&line);
        assert_eq!(style.spans(), &[(0..2, Style::Lifetime)]);
        assert_eq!(style.at(0), Style::Lifetime);
        assert_eq!(style.at(1), Style::Lifetime);
    }

    #[test]
    fn the_rust_highlighter_styles_lifetimes_in_realistic_lines() {
        let highlighter = RustSyntaxHighlighter;
        // The `&` stays plain; `'static` is a lifetime, `str` a type.
        let style =
            highlighter.base_style(&Line::from("let x: &'static str;"));
        assert_eq!(
            style.spans(),
            &[
                (0..3, Style::Keyword),
                (16..19, Style::Type),
                (8..15, Style::Lifetime),
            ]
        );
        for i in 8..15 {
            assert_eq!(style.at(i), Style::Lifetime, "grapheme {i}");
        }
        assert_eq!(style.at(7), Style::None, "the & stays plain");

        let style = highlighter.base_style(&Line::from("-> &'_ str"));
        assert_eq!(
            style.spans(),
            &[(7..10, Style::Type), (4..6, Style::Lifetime)]
        );
    }

    #[test]
    fn a_lifetime_span_wins_the_overlap_when_added_after_a_char_span() {
        // The overlap rule itself: a lifetime span added after a char
        // span wins the graphemes it covers.
        let mut layered = LineStyle::new();
        layered.add(0..4, Style::Char);
        layered.add(2..5, Style::Lifetime);
        assert_eq!(layered.at(0), Style::Char);
        assert_eq!(layered.at(2), Style::Lifetime);
        assert_eq!(layered.at(4), Style::Lifetime);

        // The same ordering the highlighter uses: the lifetime spans
        // are added after the character spans, so where they overlap
        // the lifetime wins.
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("it's");
        // The apostrophe of `it's` is the lifetime's quote; the run
        // after it is `s`, so the lifetime span is 2..4 — the same span
        // the character scan would have added if the quote had a closer.
        let style = highlighter.base_style(&line);
        assert_eq!(style.spans(), &[(2..4, Style::Lifetime)]);
        for i in 2..4 {
            assert_eq!(style.at(i), Style::Lifetime, "grapheme {i}");
        }
    }

    #[test]
    fn the_rust_highlighter_does_not_style_lifetimes_in_plain_text() {
        let highlighter = PlainTextSyntaxHighlighter;
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(
            highlighter
                .base_style(&Line::from("fn f<'a>(s: &'a str) -> &'static str"))
                .spans(),
            no_spans
        );
    }

    #[test]
    fn the_rust_highlighter_styles_keywords() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("if x { }");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(0..2, Style::Keyword)]
        );
    }

    #[test]
    fn the_rust_highlighter_styles_common_types() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("let n: u128 = 5;");
        // The digits of u128 are a number run, so they carry a Digit
        // span; the Type span of the whole word is added after it and
        // wins the overlap.
        let style = highlighter.base_style(&line);
        assert_eq!(
            style.spans(),
            &[
                (8..11, Style::Digit),
                (14..15, Style::Digit),
                (0..3, Style::Keyword),
                (7..11, Style::Type),
            ]
        );
        for i in 7..11 {
            assert_eq!(style.at(i), Style::Type, "grapheme {i} of u128");
        }
    }

    #[test]
    fn the_rust_highlighter_styles_literals_and_enum_variants() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("Some None Ok Err true false");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[
                (0..4, Style::Literal),
                (5..9, Style::Literal),
                (10..12, Style::Literal),
                (13..16, Style::Literal),
                (17..21, Style::Literal),
                (22..27, Style::Literal),
            ]
        );
    }

    #[test]
    fn the_rust_highlighter_does_not_style_a_word_glued_to_more_text() {
        let highlighter = RustSyntaxHighlighter;
        // Whole-word matching only: none of these runs equals a list
        // entry, and `False` does not match `false`.
        let line = Line::from("myself iffy xtrue Somex False");
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(highlighter.base_style(&line).spans(), no_spans);
    }

    #[test]
    fn the_rust_highlighter_does_not_style_static() {
        let highlighter = RustSyntaxHighlighter;
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(
            highlighter.base_style(&Line::from("static")).spans(),
            no_spans
        );
    }

    #[test]
    fn the_rust_highlighter_styles_a_word_glued_to_punctuation() {
        let highlighter = RustSyntaxHighlighter;
        // Punctuation is not a word character, so the run is exactly
        // `true`: it matches the literal.
        let line = Line::from("true!");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(0..4, Style::Literal)]
        );
    }

    #[test]
    fn the_rust_highlighter_styles_a_character_literal_including_its_quotes() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("'a'");
        let style = highlighter.base_style(&line);
        assert_eq!(style.spans(), &[(0..3, Style::Char)]);
        for i in 0..3 {
            assert_eq!(style.at(i), Style::Char, "grapheme {i} of 'a'");
        }
    }

    #[test]
    fn the_rust_highlighter_styles_an_escaped_quote_as_one_literal() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("'\\''");
        let style = highlighter.base_style(&line);
        // The backslash escapes the quote it precedes, so the literal
        // runs to the very last quote: four graphemes, all Char.
        assert_eq!(style.spans(), &[(0..4, Style::Char)]);
        for i in 0..4 {
            assert_eq!(style.at(i), Style::Char, "grapheme {i}");
        }
    }

    #[test]
    fn the_rust_highlighter_does_not_style_an_unpaired_quote_as_a_character() {
        let highlighter = RustSyntaxHighlighter;
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(highlighter.base_style(&Line::from("'")).spans(), no_spans);
        // A line that ends in the escape: the backslash escapes past
        // the end of the line, so there is no closer, and the backslash
        // is not a lifetime-name character, so no lifetime either.
        assert_eq!(highlighter.base_style(&Line::from("'\\")).spans(), no_spans);
    }

    #[test]
    fn the_rust_highlighter_pairs_quotes_naively_left_to_right() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("it's a 'test'");
        // Documented naive-pairing behavior: the apostrophe of `it's`
        // opens the literal and the opening quote of `'test'` closes it,
        // so the span runs from the apostrophe to that quote.
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(2..8, Style::Char)]
        );
    }

    #[test]
    fn the_rust_highlighter_adds_no_keyword_span_inside_a_character_literal() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("'if'");
        let style = highlighter.base_style(&line);
        // The whole literal is one Char span; the `if` inside it is
        // skipped by the scan, so no keyword span is added.
        assert_eq!(style.spans(), &[(0..4, Style::Char)]);
        for i in 0..4 {
            assert_eq!(style.at(i), Style::Char, "grapheme {i} of 'if'");
        }
    }

    #[test]
    fn the_rust_highlighter_styles_each_of_two_character_literals() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("'a' + 'b'");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(0..3, Style::Char), (6..9, Style::Char)]
        );
    }

    #[test]
    fn the_rust_highlighter_styles_a_multibyte_character_literal_in_grapheme_space() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("'é'");
        let style = highlighter.base_style(&line);
        // 'é' is two bytes in UTF-8 but one grapheme, so the literal is
        // three graphemes wide.
        assert_eq!(style.spans(), &[(0..3, Style::Char)]);
        assert_eq!(style.at(1), Style::Char);
    }

    #[test]
    fn the_rust_highlighter_does_not_check_the_interior_of_a_character_literal() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("'definitelynotachar'");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(0..20, Style::Char)]
        );
    }

    #[test]
    fn the_rust_highlighter_styles_a_realistic_char_binding_line() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("let c: char = 'a';");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[
                (0..3, Style::Keyword),
                (7..11, Style::Type),
                (14..17, Style::Char),
            ]
        );
    }

    #[test]
    fn a_type_name_wins_the_overlap_with_its_inner_digit_span() {
        let highlighter = RustSyntaxHighlighter;
        // The inner digits of `i8` and `u128` are number runs, so they
        // carry digit spans; the word spans are added after them, so
        // the type style wins every grapheme of each word and no digit
        // span survives inside one.
        let line = Line::from("i8 u128");
        let style = highlighter.base_style(&line);
        assert_eq!(
            style.spans(),
            &[
                (1..2, Style::Digit),
                (4..7, Style::Digit),
                (0..2, Style::Type),
                (3..7, Style::Type),
            ]
        );
        for i in 0..2 {
            assert_eq!(style.at(i), Style::Type);
        }
        for i in 3..7 {
            assert_eq!(style.at(i), Style::Type);
        }
    }

    #[test]
    fn the_plain_text_highlighter_does_not_style_words() {
        let highlighter = PlainTextSyntaxHighlighter;
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(
            highlighter
                .base_style(&Line::from("if let Some(x) = y { 1 }"))
                .spans(),
            no_spans
        );
    }

    #[test]
    fn the_plain_text_highlighter_does_not_style_character_literals() {
        let highlighter = PlainTextSyntaxHighlighter;
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(
            highlighter.base_style(&Line::from("'a' it's 'test'")).spans(),
            no_spans
        );
    }

    #[test]
    fn the_rust_highlighter_without_digits_has_no_spans() {
        // The line has no numbers and no word in a list: it stays
        // unstyled.
        let highlighter = RustSyntaxHighlighter;
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(
            highlighter.base_style(&Line::from("x = y;")).spans(),
            no_spans
        );
    }

    #[test]
    fn the_plain_text_highlighter_styles_nothing() {
        let highlighter = PlainTextSyntaxHighlighter;
        assert_eq!(highlighter.file_type(), FileType::PlainText);
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(highlighter.base_style(&Line::from("the answer is 42")).spans(), no_spans);
    }

    #[test]
    fn the_highlighter_of_a_file_type_matches_its_type() {
        assert_eq!(highlighter_for(FileType::PlainText).file_type(), FileType::PlainText);
        assert_eq!(highlighter_for(FileType::Rust).file_type(), FileType::Rust);
    }

    #[test]
    fn the_default_styles_are_styled_by_the_plain_text_highlighter() {
        let styles = Styles::default();
        assert_eq!(styles.file_type(), FileType::PlainText);
    }

    #[test]
    fn the_default_highlighter_of_a_new_line_stays_unstyled() {
        // A new document is plain text: its lines carry no base styling,
        // so a digit inserted into a new line stays unstyled until the
        // buffer is saved under a language's name.
        let mut styles = Styles::for_lines(FileType::PlainText, &[]);
        let no_spans: &[(Range<usize>, Style)] = &[];
        styles.insert_line(0, &Line::from("42"));
        assert_eq!(styles.line(0).unwrap().spans(), no_spans);
    }
}
