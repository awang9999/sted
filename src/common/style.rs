use crate::common::line::Line;
use crate::common::types::FileType;
use std::ops::Range;

/// How a span of a line's text is styled. The view renders each kind
/// differently: plain spans in the terminal's default colors, search
/// matches in the match highlight, the active match — the one the caret
/// sits on — in its own, stronger highlight, and digits in the digit
/// color.
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
    /// A digit: drawn in the digit color.
    Digit,
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

/// The styling of the lines of a buffer: one [`LineStyle`] per line,
/// kept in parallel with the buffer's lines.
///
/// This structure is the single owner of how the buffer's text is
/// styled: the digits of the text for now, and syntax coloring as it is
/// added. It stores, for each line, the styling information, and the
/// view hands a line its styling when the line builds its annotated
/// string.
///
/// The styling follows the buffer's file type: the type decides which
/// rules apply. For now, digits are styled only in Rust files, while
/// plain text carries no base styling at all.
#[derive(Clone, Debug, Default)]
pub struct Styles {
    lines: Vec<LineStyle>,
    /// The type of file the styled lines belong to: it decides which
    /// highlighting rules the base styling applies — for now, digits
    /// are styled only in Rust files, while plain text carries no base
    /// styling at all.
    file_type: FileType,
}

impl Styles {
    /// The styling for a slice of the lines of a `file_type` file: the
    /// base styling of each line, with no other styles.
    pub fn for_lines(file_type: FileType, lines: &[Line]) -> Self {
        Self {
            lines: lines
                .iter()
                .map(|line| Self::base_for(file_type, line))
                .collect(),
            file_type,
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
            .insert(row.min(self.lines.len()), Self::base_for(self.file_type, line));
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
            *style = Self::base_for(self.file_type, line);
        }
    }

    /// Updates the type the styled lines belong to. When it changes, the
    /// base styling of every line in `lines` is rebuilt under the new
    /// rules, so a re-typed buffer (a save-as to a new extension) gets
    /// its digits highlighted or un-highlighted accordingly.
    pub fn set_file_type(&mut self, file_type: FileType, lines: &[Line]) {
        if self.file_type == file_type {
            return;
        }
        self.file_type = file_type;
        self.lines = lines
            .iter()
            .map(|line| Self::base_for(file_type, line))
            .collect();
    }

    /// The base styling of a line's text under the rules of `file_type`:
    /// a digit style over each run of the line's digits, while the file
    /// type styles digits — plain text carries none, so its digits stay
    /// unstyled.
    fn base_for(file_type: FileType, line: &Line) -> LineStyle {
        let mut style = LineStyle::new();
        if file_type == FileType::Rust {
            for span in line.digit_spans() {
                style.add(span, Style::Digit);
            }
        }
        style
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
        assert_eq!(styles.file_type, FileType::PlainText);
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(styles.line(0).unwrap().spans(), no_spans);

        // ... and re-typing it to Rust restores it.
        styles.set_file_type(FileType::Rust, &lines);
        assert_eq!(styles.file_type, FileType::Rust);
        assert_eq!(styles.line(0).unwrap().spans(), &[(4..7, Style::Digit)]);
    }

    #[test]
    fn for_lines_holds_one_style_per_line() {
        let lines = [Line::from("a1"), Line::from("b2"), Line::from("c")];
        let styles = Styles::for_lines(FileType::Rust, &lines);
        assert_eq!(styles.line(0).unwrap().spans(), &[(1..2, Style::Digit)]);
        assert_eq!(styles.line(1).unwrap().spans(), &[(1..2, Style::Digit)]);
        let no_spans: &[(Range<usize>, Style)] = &[];
        assert_eq!(styles.line(2).unwrap().spans(), no_spans);
        assert_eq!(styles.line(3), None);
    }

    #[test]
    fn insert_line_places_the_base_styling_and_shifts_the_rest_down() {
        let mut styles = Styles::for_lines(FileType::Rust, &[Line::from("a1")]);
        let line = Line::from("b2");
        styles.insert_line(0, &line);
        assert_eq!(styles.line(0).unwrap().spans(), &[(1..2, Style::Digit)]);
        assert_eq!(styles.line(1).unwrap().spans(), &[(1..2, Style::Digit)]);
        assert_eq!(styles.line(2), None);
    }

    #[test]
    fn remove_line_drops_the_styling_and_shifts_the_rest_up() {
        let mut styles = Styles::for_lines(FileType::Rust, &[Line::from("a1"), Line::from("b2")]);
        styles.remove_line(0);
        assert_eq!(styles.line(0).unwrap().spans(), &[(1..2, Style::Digit)]);
        assert_eq!(styles.line(1), None);
        // Removing out of range is a no-op.
        styles.remove_line(5);
        assert_eq!(styles.line(0).unwrap().spans(), &[(1..2, Style::Digit)]);
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
        // follow the digits to their new graphemes.
        line.insert_char(0, 'x');
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
}
