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
/// drawn in the lifetime color. Comments are drawn in the comment
/// color: a single-line comment — a `//` marker and everything to the
/// end of the line, marker included — and a multi-line comment — a
/// `/* ... */` region, a span in each of the lines it covers, nested
/// `/*` markers inside it included.
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
    /// A comment: a single-line comment — the `//` marker and everything
    /// to the end of the line, marker included — or a multi-line comment
    /// — the `/* ... */` region, a span in each of the lines it covers,
    /// nested `/*` markers inside it included: drawn in the comment
    /// color.
    Comment,
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
///
/// A language whose comments can span lines — Rust, say — styles a line
/// with the depth of the comment carried over from the lines above it:
/// the number of unclosed `/*` markers. [`base_style_with_depth`](
/// SyntaxHighlighter::base_style_with_depth) takes that depth and
/// returns the depth the line leaves open for the next line. The
/// defaults keep the depth at zero: a language without such comments —
/// plain text — never carries any, so it implements only
/// [`base_style`](SyntaxHighlighter::base_style); a language with them
/// implements `base_style_with_depth`, and `base_style` then styles a
/// line as if the depth were zero.
pub trait SyntaxHighlighter: Send + Sync {
    /// The type of file this highlighter styles: the file type it is
    /// registered as the highlighter of.
    fn file_type(&self) -> FileType;

    /// The base styling of `line`'s text under this language's rules,
    /// with no multi-line comment carried over from an earlier line.
    fn base_style(&self, line: &Line) -> LineStyle {
        self.base_style_with_depth(line, 0).0
    }

    /// The base styling of `line`'s text under this language's rules,
    /// given `depth_in`, the number of unclosed `/*` markers the lines
    /// above left open: a line styled with a non-zero depth sits inside
    /// the comment carried over from an earlier line. Returns the
    /// styling and `depth_out`, the number of unclosed markers the line
    /// leaves open for the next line.
    ///
    /// The default keeps the depth at zero — in and out: a language
    /// without multi-line comments never carries any.
    fn base_style_with_depth(&self, line: &Line, depth_in: usize) -> (LineStyle, usize) {
        let _ = depth_in;
        (self.base_style(line), 0)
    }
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
///
/// Single-line comments are styled too: the first `//` in the line
/// whose first slash sits in a plain grapheme — the slashes of a
/// character literal are skipped with the literal, so the `//` inside
/// `'//'` is no marker — and everything to the end of the line, marker
/// included, carries [`Style::Comment`]. A single slash is no comment;
/// `///` doc comments and `//!` module doc comments are ordinary
/// comments with no special treatment. No other base highlighting
/// starts inside a comment: no word, character-literal, or lifetime
/// span may start at or after the marker — the comment swallows the
/// rest of the line the way a character literal swallows its
/// interior. A `//` inside a double-quoted string still starts a
/// comment: strings are otherwise unstyled by the highlighter, so
/// their slashes are plain graphemes. The comment span is added after
/// every other span, so it wins the overlap of the digit spans of any
/// numbers the comment mentions. A `//` inside a multi-line comment
/// region is no marker: the region owns those graphemes, so the code
/// after the closing `*/` is styled as usual, not swallowed by a
/// single-line comment to the end of the line.
///
/// Multi-line comments are styled too: a `/*` and the matching `*/`
/// that closes it — the region between, markers included — carries
/// [`Style::Comment`], one span in each of the lines it covers. The
/// depth of the comment is tracked the way the Rust compiler tracks
/// it: each `/*` adds one, each `*/` removes one, and the comment
/// closes at the `*/` that brings the depth back to zero — a nested
/// `/* ... */` inside a comment is part of it, not a comment of its
/// own. The counting is naive: a `/*` or a `*/` in the interior of a
/// character literal counts too, whatever the literal pairing does —
/// the markers are counted on the raw graphemes of the line. A line
/// that ends with the depth above zero leaves the comment open: the
/// depth is carried to the next line, which sits inside the comment
/// from column zero until the `*/` that closes it, and a `/*` in a
/// line with the depth above zero adds one without opening a new
/// region. A `*/` with the depth at zero is a stray marker — a
/// multiplication and a division, say — and changes nothing. No other
/// base highlighting starts inside a comment region: no word,
/// character-literal, or lifetime span starts at or after an open
/// `/*` and before its closing `*/` — the region swallows them the
/// way a single-line comment swallows the rest of the line. A line
/// that starts with the depth above zero is comment from column zero
/// to its closing `*/`, and the regular highlighting resumes after the
/// marker on the same line. An empty line inside a comment carries no
/// span — it has no graphemes, so nothing to color — but it carries
/// the depth to the next line. The comment spans are added after every
/// other span, so they win the overlap of the digit spans of any
/// numbers the comment mentions.
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

/// The state carried by the left-to-right scan of a line's graphemes
/// for the base styling: the character-literal spans found so far, the
/// multi-line comment depth, the position of the `/*` that opened the
/// current unclosed comment region, the closed comment regions found so
/// far, and the `//` marker position.
///
/// The invariant the methods keep: `depth` is the number of unclosed
/// `/*` markers — the depth carried in from the earlier lines plus one
/// per `/*` the scan saw minus one per `*/` it saw — and `ml_open` is
/// `Some` exactly when `depth` is non-zero, holding the position of the
/// `/*` that opened the region the depth counts. A line styled with a
/// non-zero depth starts inside the comment carried over from an
/// earlier line: its region starts at column zero.
struct ScanState {
    /// The character-literal spans the scan found, in line order: their
    /// interiors are never visited for words or a `//` marker, but their
    /// `/*` and `*/` markers still count toward the depth.
    char_spans: Vec<Range<usize>>,
    /// The multi-line comment depth: the depth carried in from the
    /// earlier lines, plus one per `/*` the scan saw, minus one per `*/`.
    depth: usize,
    /// The position of the `/*` that opened the current unclosed comment
    /// region: `Some` exactly when `depth` is non-zero.
    ml_open: Option<usize>,
    /// The closed comment regions found so far, in line order: each the
    /// `/*` that opened it and the `*/` that closed it, markers
    /// included.
    ml_regions: Vec<Range<usize>>,
    /// The position of the `//` marker, when the scan found one in a
    /// plain grapheme outside a comment region: from the marker to the
    /// end of the line is the single-line comment, and the scan ends
    /// with it.
    comment_start: Option<usize>,
}

impl ScanState {
    /// The state of a scan of a line styled with the depth carried in
    /// from the earlier lines: a line with a non-zero depth sits inside
    /// the comment carried over from an earlier line, which starts at
    /// column zero.
    fn new(depth_in: usize) -> Self {
        Self {
            char_spans: Vec::new(),
            depth: depth_in,
            ml_open: if depth_in == 0 { None } else { Some(0) },
            ml_regions: Vec::new(),
            comment_start: None,
        }
    }

    /// The effect of an `/*` marker at `position` on the depth: add
    /// one, and remember the position when no region is open yet — the
    /// marker opens the current comment region.
    fn see_ml_open(&mut self, position: usize) {
        if self.ml_open.is_none() {
            self.ml_open = Some(position);
        }
        self.depth = self.depth.saturating_add(1);
    }

    /// The effect of a `*/` marker at `position` on the depth: remove
    /// one, and close the current region — recorded in `ml_regions` —
    /// when the depth reaches zero. A marker with no region open — a
    /// stray `*/`, a multiplication and a division, say — changes
    /// nothing.
    fn see_ml_close(&mut self, position: usize) {
        if self.ml_open.is_some() {
            self.depth = self.depth.saturating_sub(1);
            if self.depth == 0 {
                let open = self
                    .ml_open
                    .take()
                    .expect("a non-zero depth has an open region");
                self.ml_regions.push(open..position.saturating_add(2));
            }
        }
    }

    /// The effect on the depth of the `/*` and `*/` markers in the
    /// graphemes `from` to (exclusive) `to`: the interior of a
    /// character literal is scanned for them even though its words and
    /// quotes are never visited — the naive counting.
    fn count_markers(&mut self, from: usize, to: usize, line: &Line) {
        let mut index = from;
        while index < to {
            if line.starts_ml_comment(index) {
                self.see_ml_open(index);
                index = index.saturating_add(2);
            } else if line.closes_ml_comment(index) {
                self.see_ml_close(index);
                index = index.saturating_add(2);
            } else {
                index = index.saturating_add(1);
            }
        }
    }

    /// Whether the grapheme at `position` sits inside the comment the
    /// scan has open: one carried over from an earlier line (which
    /// starts at column zero), or one the scan opened on this line.
    fn in_ml_comment(&self, position: usize) -> bool {
        self.ml_open.is_some_and(|open| position >= open)
    }

    /// Whether the grapheme at `position` sits inside any multi-line
    /// comment region of the line: the open one, or a closed one.
    fn in_ml_region(&self, position: usize) -> bool {
        self.in_ml_comment(position)
            || self.ml_regions.iter().any(|region| region.contains(&position))
    }
}

/// The position the scan resumes from after walking the graphemes from
/// `position` up to (exclusive) `limit`, left to right:
///
/// - inside the open comment region, only the markers matter: a `/*`
///   adds one to the depth, a `*/` removes one and closes the region
///   when the depth reaches zero, and a `//` there is no marker and a
///   quote opens no character literal — the region swallows them;
/// - a quote that opens a character literal claims everything up to the
///   literal's end — the grapheme just past its closing quote — and the
///   walk resumes there; the literal's interior is scanned for `/*` and
///   `*/` markers — the naive counting — but not for words or a `//`
///   marker;
/// - a `//` marker in a plain grapheme outside a comment region is
///   recorded in the state, and the walk ends there: from the marker to
///   the end of the line is the single-line comment, so no `/*` after
///   it counts toward the depth;
/// - a `/*` in a plain grapheme adds one to the depth, opening the
///   current region when the depth was zero; a `*/` with no region open
///   is a stray marker and changes nothing.
fn advance(
    position: usize,
    limit: usize,
    line: &Line,
    style: &mut LineStyle,
    state: &mut ScanState,
) -> usize {
    let mut position = position;
    while position < limit {
        if state.in_ml_comment(position) {
            if line.starts_ml_comment(position) {
                state.see_ml_open(position);
                position = position.saturating_add(2);
            } else if line.closes_ml_comment(position) {
                state.see_ml_close(position);
                position = position.saturating_add(2);
            } else {
                position = position.saturating_add(1);
            }
        } else if let Some(end) = line.char_literal_end(position) {
            style.add(position..end, Style::Char);
            state.char_spans.push(position..end);
            state.count_markers(position.saturating_add(1), end, line);
            position = end;
        } else if line.starts_comment(position) {
            state.comment_start = Some(position);
            return position;
        } else if line.starts_ml_comment(position) {
            state.see_ml_open(position);
            position = position.saturating_add(2);
        } else {
            // A stray */ with no region open, or an ordinary grapheme:
            // one step past it.
            position = position.saturating_add(1);
        }
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
    /// quote — the lifetime style over each lifetime specifier — a
    /// quote that opens no character literal, followed by a run of
    /// identifier characters — and the comment style over the line's
    /// comments: each multi-line comment region — the `/*` that opens
    /// it and the `*/` that closes it, markers included, a region that
    /// was still open at the end of the line running to the end of the
    /// line — the single-line comment — the first `//` marker whose
    /// first slash sits in a plain grapheme, and everything to the end
    /// of the line, marker included — and, when `depth_in` is
    /// non-zero, the comment carried over from the earlier lines, which
    /// runs from column zero to the `*/` that closes it.
    ///
    /// The spans land in phases, and a later span wins an overlap:
    /// first the digit spans, then the word and character-literal
    /// spans — so a type like `u128` or `i8` renders fully in the type
    /// color, over the digit spans of its inner digits — then the
    /// lifetime spans, and finally the comment spans, which win the
    /// overlap of any digit span a comment sits over. The words and the
    /// character literals are found in one scan, in line order: a word
    /// run that is in one of the lists carries that list's style, and a
    /// quote that opens a character literal hands the scan to the
    /// literal parser, whose end — the grapheme just past the closing
    /// quote — is where the scan resumes. The interior words of a
    /// literal are never visited, so no word or keyword span is ever
    /// added inside one, and a literal's span wins the overlap of any
    /// digit spans inside it. The same scan tracks the comments: a
    /// `/*` in the plain graphemes adds one to the depth carried in
    /// from the earlier lines, a `*/` removes one, and the region
    /// closes at the `*/` that brings the depth to zero; a word inside
    /// a comment region is swallowed — the region starts no word,
    /// character-literal, or lifetime span — and the first `//` marker
    /// outside a comment region ends the scan, so no `/*` after it
    /// counts toward the depth.
    fn base_style_with_depth(&self, line: &Line, depth_in: usize) -> (LineStyle, usize) {
        let mut style = LineStyle::new();
        for span in line.number_spans() {
            style.add(span, Style::Digit);
        }
        // The word and character-literal spans are added after the
        // number spans on purpose: a later span wins an overlap, so a
        // type like `u128` or `i8` renders fully in the type color,
        // over the digit spans of its inner digits.
        let mut state = ScanState::new(depth_in);
        let words = line.word_spans();
        let mut position = 0;
        for span in &words {
            position = advance(position, span.start, line, &mut style, &mut state);
            if state.comment_start.is_some() {
                // The single-line comment began in the gap before this
                // word: every word, quote, and lifetime from here on
                // sits in it, and no `/*` after the marker counts.
                break;
            }
            if position > span.start {
                // The word is inside a character literal: its opening
                // quote claimed it, so it carries no style of its own.
                continue;
            }
            if state.in_ml_region(span.start) {
                // The word is inside a multi-line comment region: the
                // region swallows it, the way a single-line comment
                // swallows the rest of the line.
                position = span.end;
                continue;
            }
            let word = line.run_text(span.start, span.end);
            if let Some(word_style) = style_of_word(word) {
                style.add(span.clone(), word_style);
            }
            position = span.end;
        }
        // A quote may open a literal in the plain tail after the last
        // word too — and so may a comment: the tail walk looks for
        // both.
        if state.comment_start.is_none() {
            advance(position, line.grapheme_count(), line, &mut style, &mut state);
        }
        // The lifetime spans are added after all the character spans,
        // and in line order, on purpose: a quote the character scan
        // left unpaired — an unclosed `'a` — still starts a lifetime
        // run, and a lifetime span that falls inside a character
        // literal is dropped instead: the literal's closing quote is
        // not the quote a lifetime opens, so a literal like `'a'` stays
        // fully a character, with no lifetime span added for the `'a`
        // inside it. A lifetime whose quote sits in a comment is
        // dropped too — in the single-line comment, which owns the
        // rest of the line and opens with a slash — no run character —
        // or in a multi-line comment region, which swallows it: no
        // lifetime run starts inside a comment. Where a lifetime span
        // does overlap another span, the later span wins the overlap,
        // as it does for the word styles.
        for span in line.lifetime_spans() {
            let in_comment =
                state.comment_start.is_some_and(|start| span.start >= start)
                    || state.in_ml_region(span.start);
            if !in_comment && !state.char_spans.iter().any(|c| c.contains(&span.start)) {
                style.add(span, Style::Lifetime);
            }
        }
        // A comment that was still open at the end of the line runs to
        // the end of the line: close the region there, leaving the
        // depth to carry over to the next line.
        if let Some(open) = state.ml_open.take() {
            state.ml_regions.push(open..line.grapheme_count());
        }
        // The comment spans are added last, after the digit, word,
        // character, and lifetime spans, on purpose: a later span wins
        // an overlap, so the comment color wins every grapheme of a
        // comment — from its opening marker, or from column zero when
        // the line sits inside the comment carried over from an
        // earlier line, to its closing marker, or to the end of the
        // line when it was not closed on this line — over the digit
        // spans of any numbers the comment mentions.
        for region in &state.ml_regions {
            style.add(region.clone(), Style::Comment);
        }
        if let Some(start) = state.comment_start {
            style.add(start..line.grapheme_count(), Style::Comment);
        }
        (style, state.depth)
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
///
/// A highlighter whose rules carry a comment depth across lines — the
/// Rust one, with its multi-line comments — needs, to re-style a line,
/// the depth that line starts at: the depth the lines above it left
/// open. `depth_after` stores that depth after every line, and `texts`
/// mirrors the lines' text, so a re-style of a line and the lines below
/// it never has to ask the buffer for anything. The invariant the edit
/// methods keep: for every `i`, `lines[i]` is the highlighter's
/// styling of `texts[i]` given the depth after the line above it (zero
/// for the first line), and `depth_after[i]` is the depth that styling
/// leaves open. An edit re-styles the edited line and the lines below
/// it until the depth reconverges with the stored value: a line that
/// starts at the depth it stored, with its text unchanged, styles
/// identically, and so does every line below it, so the re-style stops
/// there.
#[derive(Clone)]
pub struct Styles {
    /// The lines' text, kept in parallel with the buffer's lines: the
    /// mirror the edit methods update, so a re-style of a suffix never
    /// has to ask the buffer for the lines' text.
    texts: Vec<Line>,
    lines: Vec<LineStyle>,
    /// The multi-line comment depth after each line: the number of
    /// unclosed `/*` markers the lines up to and including it left
    /// open. Parallel to `lines`; zero for every line of a file whose
    /// highlighter carries no depth.
    depth_after: Vec<usize>,
    /// The highlighter whose rules the base styling of the lines is
    /// built under: the one of the buffer's file type. Held behind an
    /// [`Arc`] so a `Styles` — and with it a buffer — stays `Clone`.
    highlighter: Arc<dyn SyntaxHighlighter>,
}

/// The base styling of `lines` under `highlighter`'s rules, styled top
/// to bottom: each line is styled with the depth the lines above it
/// left open. Returns the per-line styling and the depth after each
/// line.
fn styled_by(
    highlighter: &Arc<dyn SyntaxHighlighter>,
    lines: &[Line],
) -> (Vec<LineStyle>, Vec<usize>) {
    let mut styles: Vec<LineStyle> = Vec::with_capacity(lines.len());
    let mut depth_after: Vec<usize> = Vec::with_capacity(lines.len());
    let mut depth = 0;
    for line in lines {
        let (style, out) = highlighter.base_style_with_depth(line, depth);
        styles.push(style);
        depth_after.push(out);
        depth = out;
    }
    (styles, depth_after)
}

impl Default for Styles {
    /// The styling of a new, unnamed document: no lines, and the plain
    /// text highlighter — which styles nothing — as the rules.
    fn default() -> Self {
        Self::for_lines(FileType::PlainText, &[])
    }
}

impl fmt::Debug for Styles {
    /// The lines' text, their styling, the depth after each line, and
    /// the type of the highlighter in use — the highlighter itself is a
    /// black box to the formatter.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Styles")
            .field("texts", &self.texts)
            .field("lines", &self.lines)
            .field("depth_after", &self.depth_after)
            .field("file_type", &self.highlighter.file_type())
            .finish()
    }
}

impl Styles {
    /// The styling for a slice of the lines of a `file_type` file: the
    /// base styling of each line under the file type's highlighter's
    /// rules, with no other styles, styled top to bottom: each line is
    /// styled with the depth the lines above it left open.
    pub fn for_lines(file_type: FileType, lines: &[Line]) -> Self {
        let highlighter = highlighter_for(file_type);
        let texts = lines.to_vec();
        let (lines, depth_after) = styled_by(&highlighter, &texts);
        Self {
            texts,
            lines,
            depth_after,
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
    /// buffer. The new line is styled with the depth the line above it
    /// left open, and the lines below it are re-styled until the depth
    /// reconverges with the stored value.
    pub fn insert_line(&mut self, row: usize, line: &Line) {
        let row = row.min(self.texts.len());
        // The lines above `row` are untouched by the insertion: the new
        // line starts at the depth the line above it left open, and so
        // did the line that was at `row` before the insertion.
        let depth_in = self.depth_before(row);
        self.texts.insert(row, line.clone());
        let (style, depth) = self.highlighter.base_style_with_depth(line, depth_in);
        self.lines.insert(row, style);
        self.depth_after.insert(row, depth);
        self.restyle_suffix(row.saturating_add(1), depth, depth_in);
    }

    /// Removes the styling of the line at `row`, shifting the following
    /// lines' styling up: the mirror of a line being removed from the
    /// buffer. The line that was below the removed one starts at the
    /// depth the removed line started at, and the lines below it are
    /// re-styled until the depth reconverges with the stored value.
    /// Rows out of range are ignored.
    pub fn remove_line(&mut self, row: usize) {
        if row >= self.texts.len() {
            return;
        }
        let depth_in = self.depth_before(row);
        let old_before = self.depth_after[row];
        self.texts.remove(row);
        self.lines.remove(row);
        self.depth_after.remove(row);
        self.restyle_suffix(row, depth_in, old_before);
    }

    /// Rebuilds the styling of the line at `row` from the line's current
    /// text, dropping any styles previously added to it: the mirror of
    /// the line's text being edited. The line below it — and every line
    /// below that — may start at a different depth than before, so they
    /// are re-styled until the depth reconverges with the stored value.
    /// Rows out of range are ignored.
    pub fn refresh_line(&mut self, row: usize, line: &Line) {
        if self.texts.get(row).is_none() {
            return;
        }
        let old_after = self.depth_after[row];
        self.texts[row] = line.clone();
        let (style, depth) =
            self.highlighter.base_style_with_depth(line, self.depth_before(row));
        self.lines[row] = style;
        self.depth_after[row] = depth;
        self.restyle_suffix(row.saturating_add(1), depth, old_after);
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
        self.texts = lines.to_vec();
        let (lines, depth_after) = styled_by(&self.highlighter, &self.texts);
        self.lines = lines;
        self.depth_after = depth_after;
    }

    /// The type of file the styled lines belong to: the type of the
    /// highlighter whose rules the base styling is built under.
    // Only the test suite reads it so far; the buffer keeps its own
    // copy of the type.
    #[allow(dead_code)]
    pub fn file_type(&self) -> FileType {
        self.highlighter.file_type()
    }

    /// The multi-line comment depth at the start of the line at `row`:
    /// the depth the line above it left open, or zero for the first
    /// line. `row` is at most the number of lines — the depth at the
    /// end of the document, when a line is inserted below the last one.
    fn depth_before(&self, row: usize) -> usize {
        match row {
            0 => 0,
            _ => self.depth_after[row.saturating_sub(1)],
        }
    }

    /// Re-styles the lines from `index` down while the depth they start
    /// at differs from the depth they started at before the edit:
    /// `current` is the depth the line above `index` now leaves open,
    /// and `old_before` is the depth the line at `index` started at
    /// before the edit. A line that starts at its stored depth, with its
    /// text unchanged, styles identically, so the re-style stops the
    /// first time `current` reconverges with `old_before`, and every
    /// line below `index` keeps its styling.
    fn restyle_suffix(&mut self, mut index: usize, mut current: usize, mut old_before: usize) {
        while index < self.lines.len() && current != old_before {
            let (style, depth) =
                self.highlighter.base_style_with_depth(&self.texts[index], current);
            self.lines[index] = style;
            old_before = self.depth_after[index];
            self.depth_after[index] = depth;
            current = depth;
            index = index.saturating_add(1);
        }
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
        // the integers 0 and 1; let is a keyword. The // at the end of
        // the line is a comment: it starts at the marker, runs to the
        // end of the line, is added after the digit spans, and wins
        // the overlap of the digit span of 0x1f.
        let line = Line::from("let x = 42; // 0x1f");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[
                (8..10, Style::Digit),
                (15..19, Style::Digit),
                (0..3, Style::Keyword),
                (12..19, Style::Comment),
            ]
        );
    }

    #[test]
    fn the_rust_highlighter_styles_a_comment_from_the_marker_to_the_end_of_the_line() {
        let highlighter = RustSyntaxHighlighter;
        // The marker is included, and the comment runs to the end of
        // the line.
        assert_eq!(
            highlighter.base_style(&Line::from("//")).spans(),
            &[(0..2, Style::Comment)]
        );
        let style = highlighter.base_style(&Line::from("x //"));
        assert_eq!(style.spans(), &[(2..4, Style::Comment)]);
        assert_eq!(style.at(0), Style::None, "the word before the marker stays plain");
        assert_eq!(style.at(1), Style::None, "the space before the marker stays plain");
        for i in 2..4 {
            assert_eq!(style.at(i), Style::Comment, "grapheme {i}");
        }
        // A marker glued to the word before it: the word is plain, the
        // comment starts at the marker.
        assert_eq!(
            highlighter.base_style(&Line::from("x//")).spans(),
            &[(1..3, Style::Comment)]
        );
        // An indented comment starts at its marker, not at column zero.
        assert_eq!(
            highlighter.base_style(&Line::from("    // x")).spans(),
            &[(4..8, Style::Comment)]
        );
    }

    #[test]
    fn a_comment_wins_the_overlap_of_the_digit_spans_inside_it() {
        let highlighter = RustSyntaxHighlighter;
        // 42 is a number run under the comment: the digit span is
        // added in the first phase and the comment span last, so the
        // comment color wins every grapheme of the line — the same
        // pattern as a type span winning the overlap of its inner
        // digit spans.
        let line = Line::from("// let x = 42;");
        let style = highlighter.base_style(&line);
        assert_eq!(
            style.spans(),
            &[(11..13, Style::Digit), (0..14, Style::Comment)]
        );
        for i in 0..14 {
            assert_eq!(style.at(i), Style::Comment, "grapheme {i}");
        }
    }

    #[test]
    fn the_comment_span_is_added_last_so_it_wins_the_overlaps() {
        let highlighter = RustSyntaxHighlighter;
        // The comment mentions a keyword, a type, and a number: the
        // digit spans of 128 and 42 sit under the comment, and the
        // comment span is the last span added to the line, so it wins
        // every grapheme from the marker to the end of the line.
        let line = Line::from("// let n: u128 = 42;");
        let style = highlighter.base_style(&line);
        assert_eq!(
            style.spans(),
            &[
                (11..14, Style::Digit),
                (17..19, Style::Digit),
                (0..20, Style::Comment),
            ]
        );
        assert_eq!(
            style.spans().last().map(|(_, s)| *s),
            Some(Style::Comment),
            "the comment span is the last span added"
        );
        for i in 0..20 {
            assert_eq!(style.at(i), Style::Comment, "grapheme {i}");
        }
    }

    #[test]
    fn a_single_slash_is_no_comment_and_a_division_line_keeps_its_digits() {
        let highlighter = RustSyntaxHighlighter;
        // A single slash — a division, say — starts no comment.
        assert_eq!(
            highlighter.base_style(&Line::from("1/2")).spans(),
            &[(0..1, Style::Digit), (2..3, Style::Digit)]
        );
        // A division followed by a comment: the digits keep their
        // spans under the comment, which starts at the marker.
        let line = Line::from("1/2 // half");
        let style = highlighter.base_style(&line);
        assert_eq!(
            style.spans(),
            &[(0..1, Style::Digit), (2..3, Style::Digit), (4..11, Style::Comment)]
        );
        assert_eq!(style.at(1), Style::None, "the division slash stays plain");
        for i in 4..11 {
            assert_eq!(style.at(i), Style::Comment, "grapheme {i}");
        }
    }

    #[test]
    fn a_comment_marker_inside_a_character_literal_is_no_comment() {
        let highlighter = RustSyntaxHighlighter;
        // The // of '//' sits inside the literal: the whole thing is
        // one character, and the interior is never visited, so no
        // comment starts there.
        assert_eq!(
            highlighter.base_style(&Line::from("'//'")).spans(),
            &[(0..4, Style::Char)]
        );
        // The same with a wider interior: the // is inside the
        // literal, and the interior words are never visited.
        assert_eq!(
            highlighter.base_style(&Line::from("'a//b'")).spans(),
            &[(0..6, Style::Char)]
        );
    }

    #[test]
    fn a_comment_marker_inside_a_string_starts_the_comment() {
        let highlighter = RustSyntaxHighlighter;
        // Documented naive behavior: strings are otherwise unstyled by
        // the highlighter, so the // of "a//b" sits in plain graphemes
        // and starts the comment; let is the only other span, and the
        // word runs a and b never start a span — the comment swallows
        // them.
        let line = Line::from("let s = \"a//b\";");
        let style = highlighter.base_style(&line);
        assert_eq!(
            style.spans(),
            &[(0..3, Style::Keyword), (10..15, Style::Comment)]
        );
        for i in 10..15 {
            assert_eq!(style.at(i), Style::Comment, "grapheme {i}");
        }
    }

    #[test]
    fn a_comment_swallows_the_lifetime_run_of_its_apostrophe() {
        let highlighter = RustSyntaxHighlighter;
        // Outside a comment, it's would pair the apostrophe into a
        // lifetime run; inside the comment, the whole line is comment
        // and the lifetime span is dropped instead.
        assert_eq!(
            highlighter.base_style(&Line::from("// it's")).spans(),
            &[(0..7, Style::Comment)]
        );
    }

    #[test]
    fn doc_comments_are_ordinary_comments() {
        let highlighter = RustSyntaxHighlighter;
        // /// and //! carry no special treatment: the first // starts
        // the comment, the marker included, to the end of the line.
        assert_eq!(
            highlighter.base_style(&Line::from("/// doc")).spans(),
            &[(0..7, Style::Comment)]
        );
        assert_eq!(
            highlighter.base_style(&Line::from("//! doc")).spans(),
            &[(0..7, Style::Comment)]
        );
    }

    #[test]
    fn a_search_match_over_a_comment_wins_the_comment() {
        let highlighter = RustSyntaxHighlighter;
        let line = Line::from("// let x = 42;");
        let mut style = highlighter.base_style(&line);
        // The search layer is applied after the base styling, so a
        // match over the comment wins it, as it wins every base style.
        style.add(3..10, Style::Match);
        assert_eq!(style.at(0), Style::Comment, "outside the match: the comment");
        assert_eq!(style.at(3), Style::Match, "inside the match: the match wins");
        assert_eq!(style.at(13), Style::Comment, "outside the match: the comment");
    }

    #[test]
    fn the_plain_text_highlighter_does_not_style_comments() {
        let highlighter = PlainTextSyntaxHighlighter;
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(
            highlighter
                .base_style(&Line::from("// 42 let 'a' 'static"))
                .spans(),
            no_spans
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
            // Every quote opens a lifetime run (the run after each
            // quote is not immediately closed by a quote), so no
            // character literal forms: each quote is a lifetime
            // specifier, added after the word spans.
            &[
                (0..2, Style::Keyword),
                (28..31, Style::Type),
                (45..48, Style::Type),
                (17..19, Style::Lifetime),
                (25..27, Style::Lifetime),
                (37..44, Style::Lifetime),
            ],
            &[
                (0..3, Style::Keyword),
                (16..19, Style::Type),
                (8..15, Style::Lifetime),
            ],
            // Every quote opens a lifetime run, so no character
            // literal forms: each quote is a lifetime specifier, added
            // after the word spans.
            &[
                (0..2, Style::Keyword),
                (16..19, Style::Type),
                (28..31, Style::Type),
                (5..7, Style::Lifetime),
                (13..15, Style::Lifetime),
                (25..27, Style::Lifetime),
            ],
            // Same for two generics: every quote opens a lifetime run,
            // so no character literal forms.
            &[
                (0..2, Style::Keyword),
                (20..23, Style::Type),
                (32..35, Style::Type),
                (5..7, Style::Lifetime),
                (9..11, Style::Lifetime),
                (17..19, Style::Lifetime),
                (29..31, Style::Lifetime),
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
    fn the_rust_comments_fixture_is_highlighted_line_by_line() {
        // The corpus under testdata/rust_comments.txt pins the
        // single-line comment styling: the plain, indented, doc, and
        // module doc comments, a comment carrying every other base
        // style (a keyword, a type, a number, a char literal, a
        // lifetime), code-then-comment lines, a single-slash
        // non-comment, the '//' char-literal negative, a // inside a
        // double-quoted string, an apostrophe in a comment, and a //
        // at the very end of a line. The file is plain text, so the
        // test drives the highlighter directly rather than going
        // through the file type.
        let lines: Vec<Line> =
            fixture_content("rust_comments.txt").lines().map(Line::from).collect();
        let highlighter = RustSyntaxHighlighter;
        let expected: &[&[(Range<usize>, Style)]] = &[
            &[(0..18, Style::Comment)],
            &[(4..26, Style::Comment)],
            &[(0..17, Style::Comment)],
            &[(0..24, Style::Comment)],
            // The comment mentions a keyword, a type, a number, a
            // character literal, and a lifetime: none of them starts a
            // span of its own, and the digit spans of 128 and 42 sit
            // under the comment, which is added last and wins.
            &[
                (11..14, Style::Digit),
                (17..19, Style::Digit),
                (0..32, Style::Comment),
            ],
            // Code before the marker keeps its own spans: the digit
            // and the keyword; from the marker to the end of the line
            // is comment.
            &[
                (8..9, Style::Digit),
                (0..3, Style::Keyword),
                (11..30, Style::Comment),
            ],
            // A single slash is a division, not a comment: the line
            // keeps only its digit and keyword spans.
            &[(8..10, Style::Digit), (13..14, Style::Digit), (0..3, Style::Keyword)],
            // The // of the literal sits inside it: the whole thing is
            // one character, no comment.
            &[(0..3, Style::Keyword), (8..12, Style::Char)],
            // The // of the URL sits in a double-quoted string: the
            // documented naive behavior, the string is otherwise
            // unstyled, so the marker starts the comment.
            &[(0..3, Style::Keyword), (16..24, Style::Comment)],
            // The apostrophe's lifetime run sits in the comment: it
            // is dropped, the whole line is comment.
            &[(0..17, Style::Comment)],
            // A // at the very end of the line: the marker alone is
            // a comment.
            &[(8..9, Style::Digit), (0..3, Style::Keyword), (11..13, Style::Comment)],
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
    fn the_rust_ml_comments_fixture_is_highlighted_line_by_line() {
        // The corpus under testdata/rust_ml_comments.txt pins the
        // multi-line comment styling: a comment closed on its own line,
        // a nested comment, two comments on one line with code in
        // between, a // inside a comment, a /* inside a // comment, a
        // comment left open at the end of a line, a stray */, a /* in
        // the interior of a character literal (the naive counting), a
        // quote inside a comment and a character literal after it, and
        // a comment closed on one line followed by a stray */. Each
        // line is styled with the depth carried in from no earlier
        // line, and the depth the line leaves open is pinned too. The
        // file is plain text, so the test drives the highlighter
        // directly rather than going through the file type.
        let lines: Vec<Line> =
            fixture_content("rust_ml_comments.txt").lines().map(Line::from).collect();
        let highlighter = RustSyntaxHighlighter;
        // The spans each line is styled with, in the fixture's line
        // order.
        let expected: &[&[(Range<usize>, Style)]] = &[
            // The comment is closed on the line: the region is the
            // marker and the closing marker, markers included, and the
            // code after the */ keeps its keyword and digit spans.
            &[(16..18, Style::Digit), (8..11, Style::Keyword), (0..7, Style::Comment)],
            // Nested: the depth is 1, then 2 at the inner /*, back to
            // 1 at the inner */, and 0 at the final */ — only the final
            // */ closes, and the code after it is regular.
            &[(22..24, Style::Digit), (0..17, Style::Comment)],
            // Two comments on one line: the code between them keeps its
            // spans, and both comments are closed on the line.
            &[
                (8..9, Style::Digit),
                (27..28, Style::Digit),
                (0..3, Style::Keyword),
                (19..22, Style::Keyword),
                (11..18, Style::Comment),
            ],
            // The // inside the comment region is no marker: it starts
            // no single-line comment, so the code after the */ is
            // styled as usual — its digit span survives — and the
            // comment is only the region, not the rest of the line.
            &[(17..19, Style::Digit), (0..12, Style::Comment)],
            // The /* after the // marker sits in the single-line
            // comment: the scan ends at the marker, so it counts no
            // depth — the comment is only the // to the end of the
            // line, and nothing carries to the next line.
            &[(8..9, Style::Digit), (0..3, Style::Keyword), (11..30, Style::Comment)],
            // Left open at the end of the line: the region runs to the
            // end of the line, and the depth of one carries over.
            &[(0..28, Style::Comment)],
            // A stray */ with no comment open: a multiplication and a
            // division, say — it changes no depth and no region forms.
            &[(4..5, Style::Digit)],
            // The /* in the interior of the character literal counts
            // too — the naive counting: it opens the region, the real
            // /* adds a second depth, the */ removes one, and the line
            // ends with the depth of the literal's /* still open, the
            // region running to the end of the line and swallowing the
            // words after it.
            &[
                (33..34, Style::Digit),
                (0..3, Style::Keyword),
                (8..12, Style::Char),
                (9..35, Style::Comment),
            ],
            // A quote inside the comment region starts no character
            // literal — the region swallows it — and the character
            // literal after the */ is styled as usual.
            &[(10..13, Style::Keyword), (18..21, Style::Char), (0..9, Style::Comment)],
            // Two regions on one line: the first is closed on the line,
            // and the stray */ after it changes no depth.
            &[(4..6, Style::Digit), (33..34, Style::Digit), (8..18, Style::Comment)],
        ];
        // The depth each line leaves open for the next line: one for
        // the line left open at its end (line 6) and one for the line
        // whose comment stays open through the character literal
        // (line 8); zero for the rest.
        let expected_depths: &[usize] = &[0, 0, 0, 0, 0, 1, 0, 1, 0, 0];
        assert_eq!(lines.len(), expected.len(), "the fixture's line count changed");
        assert_eq!(
            lines.len(),
            expected_depths.len(),
            "the fixture's line count changed"
        );
        for (line, (want, depth)) in
            lines.iter().zip(expected.iter().copied().zip(expected_depths))
        {
            let (style, out) = highlighter.base_style_with_depth(line, 0);
            assert_eq!(style.spans(), want, "line: {line}");
            assert_eq!(out, *depth, "line: {line}");
        }
    }

    #[test]
    fn a_line_that_starts_inside_a_ml_comment_is_comment_until_the_marker() {
        let highlighter = RustSyntaxHighlighter;
        // Styled with the depth carried over from an earlier line, the
        // line is comment from column zero to the */ that closes it,
        // and the regular highlighting resumes after the marker on the
        // same line: the keyword and the digit keep their spans.
        let line = Line::from("still in comment */ let x = 42;");
        let (style, depth) = highlighter.base_style_with_depth(&line, 1);
        assert_eq!(
            style.spans(),
            &[(28..30, Style::Digit), (20..23, Style::Keyword), (0..19, Style::Comment)]
        );
        assert_eq!(depth, 0, "the */ on the line closes the comment");
        for i in 0..19 {
            assert_eq!(style.at(i), Style::Comment, "grapheme {i} of the region");
        }
        assert_eq!(style.at(19), Style::None, "the space after the marker stays plain");
        assert_eq!(style.at(20), Style::Keyword, "the keyword after the marker");
    }

    #[test]
    fn a_slash_slash_inside_a_ml_comment_starts_no_single_line_comment() {
        let highlighter = RustSyntaxHighlighter;
        // The // of the line sits in the comment region: it starts no
        // single-line comment, so the code after the */ is styled as
        // usual — the digit keeps its span — and is not swallowed by a
        // // comment to the end of the line.
        let line = Line::from("/* a // b */ x = 42;");
        let (style, depth) = highlighter.base_style_with_depth(&line, 0);
        assert_eq!(style.spans(), &[(17..19, Style::Digit), (0..12, Style::Comment)]);
        assert_eq!(depth, 0);
        for i in 13..19 {
            assert!(style.at(i) != Style::Comment, "grapheme {i} after the marker");
        }
    }

    #[test]
    fn an_unclosed_ml_comment_at_end_of_file_is_carried_by_insertion() {
        // The buffer's newline-below-the-last-line path is an insert at
        // the end: the inserted line starts at the depth the last line
        // left open, so it sits inside the comment from column zero
        // until the */ that closes it.
        let mut styles =
            Styles::for_lines(FileType::Rust, &[Line::from("/* open"), Line::from("let a = 1;")]);
        assert_eq!(styles.line(0).unwrap().spans(), &[(0..7, Style::Comment)]);
        // The line inside the comment: the digit span of 1 sits under
        // the comment span, which is added last and wins the overlap.
        assert_eq!(
            styles.line(1).unwrap().spans(),
            &[(8..9, Style::Digit), (0..10, Style::Comment)]
        );

        // A line appended at the end continues the comment until the */.
        styles.insert_line(2, &Line::from("let b = 2; */"));
        assert_eq!(
            styles.line(2).unwrap().spans(),
            &[(8..9, Style::Digit), (0..13, Style::Comment)]
        );

        // ... and a line appended below the closed comment is regular
        // code again: the comment is closed, so the depth is zero.
        styles.insert_line(3, &Line::from("let c = 3;"));
        assert_eq!(
            styles.line(3).unwrap().spans(),
            &[(8..9, Style::Digit), (0..3, Style::Keyword)]
        );
    }

    #[test]
    fn editing_a_line_inside_a_ml_comment_restyles_the_suffix_until_it_closes() {
        // The buffer's edit path is refresh_line: closing the comment
        // early on line 1 re-styles line 2 (its */ becomes a stray) and
        // stops at the depth's reconvergence; deleting the */ on line 2
        // makes the comment run over line 3, and typing it back closes
        // it again — the suffix is re-styled in both directions.
        let mut lines = vec![
            Line::from("/* open"),
            Line::from("let a = 1;"),
            Line::from("let b = 2; */"),
            Line::from("let c = 3;"),
        ];
        let mut styles = Styles::for_lines(FileType::Rust, &lines);
        // The comment runs from line 0 to the */ on line 2: lines 1 and
        // 2 are comment — the digit spans of their numbers sit under
        // the comment spans — and line 3 is regular code.
        assert_eq!(
            styles.line(1).unwrap().spans(),
            &[(8..9, Style::Digit), (0..10, Style::Comment)]
        );
        assert_eq!(
            styles.line(2).unwrap().spans(),
            &[(8..9, Style::Digit), (0..13, Style::Comment)]
        );
        assert_eq!(
            styles.line(3).unwrap().spans(),
            &[(8..9, Style::Digit), (0..3, Style::Keyword)]
        );

        // Close the comment early on line 1: the comment region ends on
        // line 1, line 2's */ is a stray and line 2 is regular code
        // again, and line 3 keeps its regular styling — the depth
        // reconverges at line 2.
        lines[1] = Line::from("let a = 1; */");
        styles.refresh_line(1, &lines[1]);
        assert_eq!(
            styles.line(1).unwrap().spans(),
            &[(8..9, Style::Digit), (0..13, Style::Comment)]
        );
        assert_eq!(
            styles.line(2).unwrap().spans(),
            &[(8..9, Style::Digit), (0..3, Style::Keyword)]
        );
        assert_eq!(
            styles.line(3).unwrap().spans(),
            &[(8..9, Style::Digit), (0..3, Style::Keyword)]
        );

        // Re-open it on line 1: line 2 is comment again, to its */ —
        // which closes the comment — and line 3 is regular code again.
        lines[1] = Line::from("let a = 1;");
        styles.refresh_line(1, &lines[1]);
        assert_eq!(
            styles.line(1).unwrap().spans(),
            &[(8..9, Style::Digit), (0..10, Style::Comment)]
        );
        assert_eq!(
            styles.line(2).unwrap().spans(),
            &[(8..9, Style::Digit), (0..13, Style::Comment)]
        );
        assert_eq!(
            styles.line(3).unwrap().spans(),
            &[(8..9, Style::Digit), (0..3, Style::Keyword)]
        );

        // Delete the */ on line 2: the comment stays open past line 2,
        // so line 3 is comment too — the re-style runs the whole
        // suffix.
        lines[2] = Line::from("let b = 2;");
        styles.refresh_line(2, &lines[2]);
        assert_eq!(
            styles.line(2).unwrap().spans(),
            &[(8..9, Style::Digit), (0..10, Style::Comment)]
        );
        assert_eq!(
            styles.line(3).unwrap().spans(),
            &[(8..9, Style::Digit), (0..10, Style::Comment)]
        );

        // Type the */ back on line 2: the comment closes there, and
        // line 3 is regular code again.
        lines[2] = Line::from("let b = 2; */");
        styles.refresh_line(2, &lines[2]);
        assert_eq!(
            styles.line(2).unwrap().spans(),
            &[(8..9, Style::Digit), (0..13, Style::Comment)]
        );
        assert_eq!(
            styles.line(3).unwrap().spans(),
            &[(8..9, Style::Digit), (0..3, Style::Keyword)]
        );
    }

    #[test]
    fn editing_lines_outside_a_ml_comment_keeps_its_depth_and_spans() {
        // Inserting, removing, and refreshing lines above an open
        // comment must not change the comment's spans or the depth it
        // leaves open: the lines above it are untouched by the comment's
        // depth, and the comment's own lines keep their styling.
        let mut lines = vec![
            Line::from("let z = 9;"),
            Line::from("/* open"),
            Line::from("let a = 1;"),
            Line::from("let b = 2; */"),
            Line::from("let c = 3;"),
        ];
        let mut styles = Styles::for_lines(FileType::Rust, &lines);
        assert_eq!(styles.line(1).unwrap().spans(), &[(0..7, Style::Comment)]);
        assert_eq!(
            styles.line(2).unwrap().spans(),
            &[(8..9, Style::Digit), (0..10, Style::Comment)]
        );
        assert_eq!(
            styles.line(3).unwrap().spans(),
            &[(8..9, Style::Digit), (0..13, Style::Comment)]
        );
        assert_eq!(
            styles.line(4).unwrap().spans(),
            &[(8..9, Style::Digit), (0..3, Style::Keyword)]
        );

        // Refreshing the line above the comment: the comment's lines
        // and the line below it keep their spans.
        lines[0] = Line::from("let z = 10;");
        styles.refresh_line(0, &lines[0]);
        assert_eq!(
            styles.line(0).unwrap().spans(),
            &[(8..10, Style::Digit), (0..3, Style::Keyword)]
        );
        assert_eq!(styles.line(1).unwrap().spans(), &[(0..7, Style::Comment)]);
        assert_eq!(
            styles.line(2).unwrap().spans(),
            &[(8..9, Style::Digit), (0..10, Style::Comment)]
        );
        assert_eq!(
            styles.line(3).unwrap().spans(),
            &[(8..9, Style::Digit), (0..13, Style::Comment)]
        );
        assert_eq!(
            styles.line(4).unwrap().spans(),
            &[(8..9, Style::Digit), (0..3, Style::Keyword)]
        );

        // Removing the line above the comment: the comment shifts up a
        // row, and the line below it — now at the row of the comment's
        // closing line — keeps its spans.
        lines.remove(0);
        styles.remove_line(0);
        assert_eq!(styles.line(0).unwrap().spans(), &[(0..7, Style::Comment)]);
        assert_eq!(
            styles.line(1).unwrap().spans(),
            &[(8..9, Style::Digit), (0..10, Style::Comment)]
        );
        assert_eq!(
            styles.line(2).unwrap().spans(),
            &[(8..9, Style::Digit), (0..13, Style::Comment)]
        );
        assert_eq!(
            styles.line(3).unwrap().spans(),
            &[(8..9, Style::Digit), (0..3, Style::Keyword)]
        );

        // Inserting a line above the comment: the comment shifts down a
        // row, and its lines keep their spans.
        lines.insert(0, Line::from("let y = 8;"));
        styles.insert_line(0, &lines[0]);
        assert_eq!(
            styles.line(0).unwrap().spans(),
            &[(8..9, Style::Digit), (0..3, Style::Keyword)]
        );
        assert_eq!(styles.line(1).unwrap().spans(), &[(0..7, Style::Comment)]);
        assert_eq!(
            styles.line(2).unwrap().spans(),
            &[(8..9, Style::Digit), (0..10, Style::Comment)]
        );
        assert_eq!(
            styles.line(3).unwrap().spans(),
            &[(8..9, Style::Digit), (0..13, Style::Comment)]
        );
        assert_eq!(
            styles.line(4).unwrap().spans(),
            &[(8..9, Style::Digit), (0..3, Style::Keyword)]
        );
    }

    #[test]
    fn an_empty_line_inside_a_ml_comment_carries_the_depth_but_no_spans() {
        // An empty line has no graphemes, so no span — it renders
        // plain, nothing to color — but it carries the depth to the
        // next line, where the comment closes.
        let lines = [
            Line::from("/* open"),
            Line::from(""),
            Line::from("*/ let x = 1;"),
        ];
        let styles = Styles::for_lines(FileType::Rust, &lines);
        assert_eq!(styles.line(0).unwrap().spans(), &[(0..7, Style::Comment)]);
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(styles.line(1).unwrap().spans(), no_spans);
        assert_eq!(
            styles.line(2).unwrap().spans(),
            &[(11..12, Style::Digit), (3..6, Style::Keyword), (0..2, Style::Comment)]
        );
    }

    #[test]
    fn a_close_marker_inside_a_char_literal_closes_a_carried_ml_comment() {
        let highlighter = RustSyntaxHighlighter;
        // Styled with the depth carried over from an earlier line, the
        // */ in the interior of the character literal counts — the
        // naive counting — and closes the comment: the region runs from
        // column zero to the marker, and the rest of the line is
        // regular code. The quote that opens the literal sits in the
        // comment region, so it starts no literal.
        let line = Line::from("let c = '*/';");
        let (style, depth) = highlighter.base_style_with_depth(&line, 1);
        assert_eq!(style.spans(), &[(0..11, Style::Comment)]);
        assert_eq!(depth, 0, "the */ in the literal's interior closes the comment");
    }

    #[test]
    fn the_plain_text_highlighter_carries_no_ml_comment_depth() {
        let highlighter = PlainTextSyntaxHighlighter;
        // Plain text styles nothing, and its depth stays zero: a line
        // that opens and closes a comment on its own leaves none open.
        let (style, depth) =
            highlighter.base_style_with_depth(&Line::from("/* a */ let x = 1;"), 0);
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(style.spans(), no_spans);
        assert_eq!(depth, 0);
        // And a depth carried in from an earlier line — impossible for
        // plain text, but the default must stay total — leaves zero.
        let (style, depth) = highlighter.base_style_with_depth(&Line::from("let x = 1;"), 3);
        assert_eq!(style.spans(), no_spans);
        assert_eq!(depth, 0);
    }

    #[test]
    fn set_file_type_rebuilds_the_styling_under_the_new_rules() {
        // A Rust file with an open comment, re-typed to plain text: the
        // comment spans are gone — plain text carries no depth at all —
        // and re-typing it to Rust restores them, with the depth
        // carried to the last line.
        let lines = [Line::from("/* open"), Line::from("let a = 1;")];
        let mut styles = Styles::for_lines(FileType::Rust, &lines);
        assert_eq!(styles.line(0).unwrap().spans(), &[(0..7, Style::Comment)]);
        assert_eq!(
            styles.line(1).unwrap().spans(),
            &[(8..9, Style::Digit), (0..10, Style::Comment)]
        );

        styles.set_file_type(FileType::PlainText, &lines);
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(styles.line(0).unwrap().spans(), no_spans);
        assert_eq!(styles.line(1).unwrap().spans(), no_spans);

        styles.set_file_type(FileType::Rust, &lines);
        assert_eq!(styles.line(0).unwrap().spans(), &[(0..7, Style::Comment)]);
        assert_eq!(
            styles.line(1).unwrap().spans(),
            &[(8..9, Style::Digit), (0..10, Style::Comment)]
        );
    }

    #[test]
    fn the_styling_demo_ml_comment_lines_are_highlighted_as_the_demo_claims() {
        // The visual demo under testdata/styling_demo.rs must stay true
        // under the highlighter's actual spans: every line of the
        // multi-line comment demo is pinned here, styled top to bottom
        // with the depth carried over from the lines above it, as the
        // buffer styles them.
        let content = fixture_content("styling_demo.rs");
        let lines: Vec<Line> = content.lines().map(Line::from).collect();
        let highlighter = RustSyntaxHighlighter;
        let mut depth = 0;
        let mut styles: Vec<(LineStyle, usize)> = Vec::new();
        for line in &lines {
            let (style, out) = highlighter.base_style_with_depth(line, depth);
            depth = out;
            styles.push((style, out));
        }
        let opening = lines
            .iter()
            .position(|line| line.to_string() == "    /* a comment spanning three lines,")
            .expect("the demo's multi-line comment opening");
        let closing = opening.saturating_add(2);
        // The opening line: the comment region runs from the marker to
        // the end of the line, and the depth of one carries over.
        assert_eq!(
            styles[opening].0.spans(),
            &[(4..38, Style::Comment)],
            "the opening line of the demo's multi-line comment"
        );
        assert_eq!(styles[opening].1, 1);
        // The line inside the comment: comment from column zero to the
        // end of the line, the digit span of 42 under it, the depth
        // still carried.
        assert_eq!(
            styles[opening.saturating_add(1)].0.spans(),
            &[(18..20, Style::Digit), (0..39, Style::Comment)],
            "the line inside the demo's multi-line comment"
        );
        assert_eq!(styles[opening.saturating_add(1)].1, 1);
        // The line that carries the nested comment and the code after
        // the closing marker: the region is comment from column zero to
        // the final */, the code after it is regular — keyword, type,
        // digit — and the trailing // after the code is its own
        // single-line comment; the depth is closed.
        assert_eq!(
            styles[closing].0.spans(),
            &[
                (51..54, Style::Digit),
                (57..58, Style::Digit),
                (43..46, Style::Keyword),
                (50..54, Style::Type),
                (0..42, Style::Comment),
                (60..75, Style::Comment),
            ],
            "the demo's nested comment line and the code after its */"
        );
        assert_eq!(styles[closing].1, 0);
        // The line below the comment is regular code with no spans,
        // and the file leaves no comment open at the end.
        assert_eq!(styles[closing.saturating_add(1)].0.spans(), &[]);
        assert_eq!(depth, 0, "the demo's file leaves no multi-line comment open");
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
        // opens the literal. The opening quote of `'test'` is no
        // lifetime run (its run `test` is immediately closed by a
        // quote), so it closes the literal; the span runs from the
        // apostrophe to that quote.
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(2..8, Style::Char)]
        );
    }

    #[test]
    fn the_rust_highlighter_does_not_pair_a_quote_with_a_lifetime_quote() {
        let highlighter = RustSyntaxHighlighter;
        // Every quote in this line opens a lifetime run: the run after
        // each quote is not immediately closed by a quote, so no quote
        // is taken as a character-literal closer. No character span
        // forms; each quote keeps its lifetime style.
        let line = Line::from("fn lifetime_demo<'a>(s: &'a str) -> &'static str {");
        let style = highlighter.base_style(&line);
        let has_char = style.spans().iter().any(|(_, s)| *s == Style::Char);
        assert!(!has_char, "no char span should form");
        // The three lifetime specifiers, in line order.
        assert_eq!(
            style.spans(),
            &[
                (0..2, Style::Keyword),
                (28..31, Style::Type),
                (45..48, Style::Type),
                (17..19, Style::Lifetime),
                (25..27, Style::Lifetime),
                (37..44, Style::Lifetime),
            ]
        );
        assert_eq!(style.at(17), Style::Lifetime, "the ' of the first 'a");
        assert_eq!(style.at(25), Style::Lifetime, "the ' of the second 'a");
        assert_eq!(style.at(37), Style::Lifetime, "the ' of 'static");
        // The stretch between the generics and the return type is not
        // swallowed as a character literal: the `>` and the `&` stay
        // plain.
        assert_eq!(style.at(19), Style::None, "the > after 'a");
        assert_eq!(style.at(36), Style::None, "the & before 'static");
    }

    #[test]
    fn the_rust_highlighter_keeps_a_char_literal_whose_closer_is_a_plain_quote() {
        let highlighter = RustSyntaxHighlighter;
        // The closer of 'a' is a plain quote (a space after it, not a
        // lifetime run), so the literal is intact; the following quote
        // starts a lifetime.
        let line = Line::from("'a' 'b");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(0..3, Style::Char), (4..6, Style::Lifetime)]
        );
        // 'static' is one character literal: the interior `static` is
        // immediately closed by a quote, so its opening quote is no
        // lifetime run, and the whole thing is a Char span.
        let line = Line::from("'static'");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(0..8, Style::Char)]
        );
        // A quoted quote: the backslash skips the escaped quote, and
        // the last quote is a plain closer.
        let line = Line::from("'\\'' ");
        assert_eq!(
            highlighter.base_style(&line).spans(),
            &[(0..4, Style::Char)]
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
