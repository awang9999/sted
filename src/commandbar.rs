use crate::common::display::fit_to_width;
use crate::common::layout::{BarLayout, MESSAGE_BAR_OFFSET};
use crate::common::line::Line;
use crate::editorcommand::{Direction, EditorCommand};
use crate::terminal::{Position, Size, Terminal};
use unicode_width::UnicodeWidthStr;

/// Shown in place of the elided beginning of text that is longer than the bar.
const ELLIPSIS: char = '…';

/// The outcome of handling a command with an open prompt: the prompt
/// either stays open, is confirmed with the entered text, or is dismissed.
#[derive(Debug, PartialEq, Eq)]
pub enum CommandBarOutcome {
    /// The prompt stays open and nothing else needs to happen.
    Stay,
    /// The user confirmed the prompt (Enter) with the entered text.
    Confirmed(String),
    /// The user dismissed the prompt (Esc or quit) without confirming.
    Cancelled,
}

/// A single-line input prompt rendered on a reserved row at the bottom of
/// the terminal: the prompt text, the text the user is entering, and the
/// caret.
///
/// The bar is prompt-agnostic — the prompt text is given to
/// [`CommandBar::new`] — so the same bar can collect a file name, a
/// search pattern, or any other single-line input. While it is active it
/// is modal: [`CommandBar::handle_command`] interprets every command
/// instead of letting it reach the view.
pub struct CommandBar {
    /// The prompt drawn in front of the text the user enters.
    prompt: String,
    line: Line,
    /// The grapheme index of the caret within `line`.
    caret: usize,
    layout: BarLayout,
}

impl CommandBar {
    /// Creates a prompt on the row the message bar occupies, so the two
    /// bars never share the screen at the same time.
    pub fn new(size: Size, prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            line: Line::newline(),
            caret: 0,
            layout: BarLayout::new(size, MESSAGE_BAR_OFFSET),
        }
    }

    /// Pre-fills the prompt with `text`, with the caret at the end of it.
    pub fn with_initial_text(mut self, text: &str) -> Self {
        self.line = Line::from(text);
        self.caret = self.line.grapheme_count();
        self
    }

    pub fn resize(&mut self, size: Size) {
        self.layout.resize(size);
    }

    /// Handles an editor command while the prompt is open and reports the
    /// outcome the caller must act on. Every command is interpreted by the
    /// prompt instead of reaching the view, so the prompt is modal.
    ///
    /// Only Enter confirms the prompt; any other command keeps it open or
    /// dismisses it (Esc and quit). A confirm with no text typed yet is
    /// ignored, so the prompt stays open.
    pub fn handle_command(&mut self, command: EditorCommand) -> CommandBarOutcome {
        match command {
            EditorCommand::Insert(c) => {
                self.insert_char(c);
                CommandBarOutcome::Stay
            }
            EditorCommand::Move(direction) => {
                self.move_caret(direction);
                CommandBarOutcome::Stay
            }
            EditorCommand::BackSpace => {
                self.backspace();
                CommandBarOutcome::Stay
            }
            EditorCommand::Delete => {
                self.delete();
                CommandBarOutcome::Stay
            }
            // Enter confirms the prompt with the typed text.
            EditorCommand::Enter => {
                if self.is_empty() {
                    CommandBarOutcome::Stay
                } else {
                    CommandBarOutcome::Confirmed(self.get_text())
                }
            }
            // Esc and Quit both dismiss the prompt without confirming.
            EditorCommand::Escape | EditorCommand::Quit => CommandBarOutcome::Cancelled,
            // Save and Find are meaningless to the prompt itself: the
            // caller decides whether it acts on the open prompt.
            EditorCommand::Save | EditorCommand::Find => CommandBarOutcome::Stay,
            EditorCommand::Resize(size) => {
                self.resize(size);
                CommandBarOutcome::Stay
            }
        }
    }

    /// Inserts a character at the caret and moves the caret one grapheme
    /// right, the way `View::handle_insert` does for the buffer.
    fn insert_char(&mut self, c: char) {
        self.line.insert_char(self.caret, c);
        self.caret = self.caret.saturating_add(1);
    }

    /// Deletes the grapheme to the left of the caret and moves the caret
    /// one grapheme left.
    fn backspace(&mut self) {
        if self.caret > 0 {
            self.line.delete_grapheme(self.caret.saturating_sub(1));
            self.caret = self.caret.saturating_sub(1);
        }
    }

    /// Deletes the grapheme at the caret (forward delete).
    fn delete(&mut self) {
        if self.caret < self.line.grapheme_count() {
            self.line.delete_grapheme(self.caret);
        }
    }

    /// Moves the caret. The bar holds a single line of text, so only
    /// horizontal directions do anything.
    fn move_caret(&mut self, direction: Direction) {
        let count = self.line.grapheme_count();
        self.caret = match direction {
            Direction::Left => self.caret.saturating_sub(1),
            Direction::Right => self.caret.saturating_add(1).min(count),
            Direction::Home => 0,
            Direction::End => count,
            // A single-line bar has no vertical movement.
            Direction::Up | Direction::Down | Direction::PageUp | Direction::PageDown => self.caret,
        };
    }

    /// The text the user entered, without the prompt.
    pub fn get_text(&self) -> String {
        self.line.to_string()
    }

    pub fn is_empty(&self) -> bool {
        self.line.grapheme_count() == 0
    }

    /// The on-screen position of the caret, measured from the top left of
    /// the terminal, the way `View::get_caret_position` does it for the
    /// buffer.
    pub fn get_caret_position(&self) -> Position {
        let (visible_from, elided) = self.visible_range();
        let prompt_width = self.prompt.width();
        let col = if !elided {
            prompt_width.saturating_add(self.line.width_until(self.caret))
        } else if self.caret < visible_from {
            // The caret sits in the elided prefix: park it just behind the
            // ellipsis.
            prompt_width.saturating_add(1)
        } else {
            prompt_width.saturating_add(1).saturating_add(
                self.line
                    .width_until(self.caret)
                    .saturating_sub(self.line.width_until(visible_from)),
            )
        };
        Position {
            col: col.min(self.layout.width().saturating_sub(1)),
            row: self.layout.row(),
        }
    }

    /// Draws the bar's row, optionally with a trailing note (like
    /// "(No matches)") right of the typed text, after the caret.
    pub fn render(&self, trailing: Option<&str>) {
        let row = self.layout.row();
        let text = self.as_line(trailing);
        let _ = Terminal::clear_row(row);
        let result = Terminal::print_row(row, &text);
        debug_assert!(result.is_ok(), "Failed to render command bar");
    }

    /// The bar's row as it will be drawn: the prompt, an ellipsis when the
    /// text does not fit, as much of the text's tail as the bar holds, and
    /// — when given — `trailing` right of the typed text. The note is
    /// display-only: it never affects the caret position or which part of
    /// the text is elided.
    fn as_line(&self, trailing: Option<&str>) -> String {
        let (visible_from, elided) = self.visible_range();
        let mut text = self.prompt.clone();
        if elided {
            text.push(ELLIPSIS);
        }
        text.push_str(&self.line.display_text_from(visible_from));
        if let Some(trailing) = trailing {
            // The note is separated from the row's content by a space,
            // unless the row holds only the prompt.
            if text.width() > self.prompt.width() {
                text.push(' ');
            }
            text.push_str(trailing);
        }
        fit_to_width(&text, self.layout.width())
    }

    /// The first grapheme index still visible in the bar, and whether the
    /// text before it is elided with an ellipsis.
    ///
    /// When the text is longer than the space next to the prompt, the bar
    /// keeps the tail (where the end of the name and the caret live) and
    /// elides the head, reserving one column for the ellipsis itself.
    fn visible_range(&self) -> (usize, bool) {
        let count = self.line.grapheme_count();
        let total = self.line.width_until(count);
        let available = self.layout.width().saturating_sub(self.prompt.width());
        if count == 0 || total <= available {
            return (0, false);
        }

        // Columns the tail of the text may occupy once the ellipsis has
        // taken one column of its own.
        let tail_columns = available.saturating_sub(1);
        let start_column = total.saturating_sub(tail_columns);

        // First grapheme that starts at or after `start_column`. Any
        // grapheme straddling that boundary stays elided.
        let mut index = 0;
        while index < count && self.line.width_until(index) < start_column {
            index = index.saturating_add(1);
        }
        (index, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The prompt every test prompt is opened with.
    const PROMPT: &str = "Save as: ";

    fn bar(width: usize) -> CommandBar {
        CommandBar::new(Size { width, height: 24 }, PROMPT)
    }

    fn bar_with_text(text: &str, width: usize) -> CommandBar {
        let mut bar = bar(width);
        for c in text.chars() {
            bar.insert_char(c);
        }
        bar
    }

    #[test]
    fn new_bar_is_empty_and_sits_on_the_message_bar_row() {
        let bar = bar(80);
        assert!(bar.is_empty());
        assert_eq!(bar.get_text(), "");
        assert_eq!(bar.layout.row(), 23);
    }

    #[test]
    fn new_bar_caret_starts_at_the_prompt() {
        let bar = bar(80);
        assert_eq!(
            bar.get_caret_position(),
            Position {
                col: PROMPT.len(),
                row: 23,
            }
        );
    }

    #[test]
    fn with_initial_text_prefills_the_bar_and_parks_the_caret_at_the_end() {
        let bar = bar(80).with_initial_text("file.txt");
        assert_eq!(bar.get_text(), "file.txt");
        assert_eq!(
            bar.get_caret_position(),
            Position {
                col: PROMPT.len() + 8,
                row: 23,
            }
        );
    }

    #[test]
    fn insert_char_appends_and_moves_the_caret_right() {
        let mut bar = bar(80);
        bar.insert_char('a');
        bar.insert_char('b');
        assert_eq!(bar.get_text(), "ab");
        assert_eq!(bar.get_caret_position().col, PROMPT.len() + 2);
    }

    #[test]
    fn insert_char_in_the_middle_keeps_surrounding_text() {
        let mut bar = bar_with_text("ac", 80);
        bar.move_caret(Direction::Home);
        bar.move_caret(Direction::Right); // caret between 'a' and 'c'
        bar.insert_char('b');
        assert_eq!(bar.get_text(), "abc");
        // The caret moved one grapheme right, behind the inserted 'b'.
        assert_eq!(bar.get_caret_position().col, PROMPT.len() + 2);
    }

    #[test]
    fn backspace_deletes_the_grapheme_left_of_the_caret() {
        let mut bar = bar_with_text("abc", 80);
        bar.backspace();
        assert_eq!(bar.get_text(), "ab");
        assert_eq!(bar.get_caret_position().col, PROMPT.len() + 2);
    }

    #[test]
    fn backspace_at_the_start_of_the_text_is_a_noop() {
        let mut bar = bar_with_text("abc", 80);
        bar.move_caret(Direction::Home);
        bar.backspace();
        assert_eq!(bar.get_text(), "abc");
    }

    #[test]
    fn delete_removes_the_grapheme_at_the_caret() {
        let mut bar = bar_with_text("abc", 80);
        bar.move_caret(Direction::Home);
        bar.delete();
        assert_eq!(bar.get_text(), "bc");
        // The caret stays put.
        assert_eq!(bar.get_caret_position().col, PROMPT.len());
    }

    #[test]
    fn delete_at_the_end_of_the_text_is_a_noop() {
        let mut bar = bar_with_text("abc", 80);
        bar.delete();
        assert_eq!(bar.get_text(), "abc");
    }

    #[test]
    fn move_caret_walks_the_text_horizontally() {
        let mut bar = bar_with_text("abcd", 80);
        let base = PROMPT.len();
        bar.move_caret(Direction::Home);
        assert_eq!(bar.get_caret_position().col, base);
        bar.move_caret(Direction::Right);
        assert_eq!(bar.get_caret_position().col, base + 1);
        bar.move_caret(Direction::Right);
        assert_eq!(bar.get_caret_position().col, base + 2);
        bar.move_caret(Direction::Left);
        assert_eq!(bar.get_caret_position().col, base + 1);
        bar.move_caret(Direction::End);
        assert_eq!(bar.get_caret_position().col, base + 4);
    }

    #[test]
    fn move_caret_clamps_at_both_ends_of_the_text() {
        let mut bar = bar_with_text("ab", 80);
        bar.move_caret(Direction::Home);
        bar.move_caret(Direction::Left);
        assert_eq!(bar.get_caret_position().col, PROMPT.len());
        bar.move_caret(Direction::End);
        bar.move_caret(Direction::Right);
        assert_eq!(bar.get_caret_position().col, PROMPT.len() + 2);
    }

    #[test]
    fn vertical_movements_do_not_move_the_caret() {
        let mut bar = bar_with_text("ab", 80);
        // The caret starts at the end of the text; Right clamps there.
        bar.move_caret(Direction::Right);
        for direction in [
            Direction::Up,
            Direction::Down,
            Direction::PageUp,
            Direction::PageDown,
        ] {
            bar.move_caret(direction);
            assert_eq!(bar.get_caret_position().col, PROMPT.len() + 2);
        }
    }

    #[test]
    fn caret_position_counts_a_tab_as_tab_width_spaces() {
        let mut bar = bar(80);
        bar.insert_char('\t');
        bar.insert_char('a');
        assert_eq!(
            bar.get_caret_position().col,
            PROMPT.len() + crate::common::constants::TAB_WIDTH_SPACES + 1
        );
    }

    #[test]
    fn as_line_shows_the_prompt_followed_by_the_text() {
        let bar = bar_with_text("abc", 80);
        assert_eq!(bar.as_line(None), "Save as: abc");
    }

    #[test]
    fn as_line_is_untouched_when_the_text_fits_exactly() {
        // 9 prompt columns + 3 text columns.
        let bar = bar_with_text("abc", 12);
        assert_eq!(bar.as_line(None), "Save as: abc");
    }

    #[test]
    fn as_line_elides_the_head_of_long_text() {
        // 16 text columns in 11 available: 1 column for the ellipsis, 10
        // for the tail.
        let bar = bar_with_text("0123456789abcdef", 20);
        assert_eq!(bar.as_line(None), "Save as: …6789abcdef");
    }

    #[test]
    fn as_line_elides_a_full_width_grapheme_that_straddles_the_boundary() {
        // Columns: a=0, b=1, 中=2..3, c=4, d=5, e=6; 3 columns available
        // (2 for the tail) start at column 5, so 中 and c are elided.
        let bar = bar_with_text("ab中cde", 12);
        assert_eq!(bar.as_line(None), "Save as: …de");
    }

    #[test]
    fn as_line_keeps_a_full_width_grapheme_that_ends_at_the_boundary() {
        // Columns: a=0, b=1, 中=2..4, c=4, d=5, e=6; 4 columns available
        // (3 for the tail) start at column 4, where 中 ends exactly, so 中
        // is elided but c..e fit.
        let bar = bar_with_text("ab中cde", 13);
        assert_eq!(bar.as_line(None), "Save as: …cde");
    }

    #[test]
    fn as_line_keeps_a_full_width_grapheme_that_starts_at_the_boundary() {
        // Columns: a=0, b=1, c=2, d=3, 中=4..6, e=6, f=7; 5 columns
        // available (4 for the tail) start at column 4, exactly where 中
        // begins, so 中 fits entirely and is kept.
        let bar = bar_with_text("abcd中ef", 14);
        assert_eq!(bar.as_line(None), "Save as: …中ef");
    }

    #[test]
    fn as_line_renders_tabs_as_spaces() {
        let bar = bar_with_text("\ta", 80);
        assert_eq!(bar.as_line(None), "Save as:     a");
    }

    #[test]
    fn as_line_is_truncated_to_the_bar_width() {
        // Narrower than the prompt itself: nothing of the prompt's tail or
        // the text can fit.
        let bar = bar_with_text("abcdef", 4);
        assert_eq!(bar.as_line(None), "Save");
    }

    #[test]
    fn as_line_appends_the_trailing_note_right_of_the_text() {
        let bar = bar_with_text("abc", 80);
        assert_eq!(
            bar.as_line(Some("(No matches)")),
            "Save as: abc (No matches)"
        );
    }

    #[test]
    fn as_line_appends_the_trailing_note_after_an_empty_text() {
        let bar = bar(80);
        assert_eq!(bar.as_line(Some("(No matches)")), "Save as: (No matches)");
    }

    #[test]
    fn the_trailing_note_does_not_change_which_text_is_visible() {
        // 16 text columns in 11 available: the same tail is elided with or
        // without the note; at this width the note simply does not fit.
        let bar = bar_with_text("0123456789abcdef", 20);
        assert_eq!(bar.as_line(Some("(No matches)")), bar.as_line(None));
        assert_eq!(bar.as_line(Some("(No matches)")), "Save as: …6789abcdef");
    }

    #[test]
    fn as_line_clips_the_trailing_note_to_the_bar_width() {
        // 9 prompt + 16 text + 13 note = 38 columns; 33 leaves the note
        // seven columns short.
        let bar = bar_with_text("0123456789abcdef", 33);
        assert_eq!(
            bar.as_line(Some("(No matches)")),
            "Save as: 0123456789abcdef (No mat"
        );
    }

    #[test]
    fn caret_position_tracks_the_unelided_text() {
        let bar = bar_with_text("abcd", 80);
        assert_eq!(
            bar.get_caret_position(),
            Position {
                col: PROMPT.len() + 4,
                row: 23,
            }
        );
    }

    #[test]
    fn caret_in_the_elided_prefix_sits_behind_the_ellipsis() {
        let mut bar = bar_with_text("abcdefghij", 12);
        bar.move_caret(Direction::Home);
        assert_eq!(
            bar.get_caret_position(),
            Position {
                col: PROMPT.len() + 1,
                row: 23,
            }
        );
    }

    #[test]
    fn caret_in_the_visible_tail_is_offset_by_the_ellipsis() {
        let bar = bar_with_text("abcdefghij", 12);
        // The visible tail is "…ij": i at prompt+2, j at prompt+3, caret
        // after j at prompt+4, clamped to the last column of the bar.
        assert_eq!(
            bar.get_caret_position(),
            Position {
                col: 12usize.saturating_sub(1),
                row: 23,
            }
        );
    }

    #[test]
    fn resize_moves_the_bar() {
        let mut bar = bar(80);
        bar.resize(Size {
            width: 100,
            height: 30,
        });
        assert_eq!(bar.layout.row(), 29);
        assert_eq!(bar.get_caret_position().row, 29);
    }

    #[test]
    fn bar_row_saturates_when_the_terminal_is_too_short() {
        let bar = CommandBar::new(
            Size {
                width: 80,
                height: 0,
            },
            PROMPT,
        );
        assert_eq!(bar.layout.row(), 1);
    }

    #[test]
    fn handle_command_inserts_text_and_keeps_the_prompt_open() {
        let mut bar = bar(80);

        let outcome = bar.handle_command(EditorCommand::Insert('a'));

        assert_eq!(outcome, CommandBarOutcome::Stay);
        assert_eq!(bar.get_text(), "a");
    }

    #[test]
    fn handle_command_enter_with_text_confirms_with_the_entered_text() {
        let mut bar = bar_with_text("file.txt", 80);

        let outcome = bar.handle_command(EditorCommand::Enter);

        assert_eq!(
            outcome,
            CommandBarOutcome::Confirmed(String::from("file.txt"))
        );
    }

    #[test]
    fn handle_command_enter_without_text_keeps_the_prompt_open() {
        let mut bar = bar(80);

        let outcome = bar.handle_command(EditorCommand::Enter);

        assert_eq!(outcome, CommandBarOutcome::Stay);
    }

    #[test]
    fn handle_command_escape_dismisses_the_prompt() {
        let mut bar = bar_with_text("file.txt", 80);

        let outcome = bar.handle_command(EditorCommand::Escape);

        assert_eq!(outcome, CommandBarOutcome::Cancelled);
    }

    #[test]
    fn handle_command_resize_resizes_the_bar() {
        let mut bar = bar(80);

        let outcome = bar.handle_command(EditorCommand::Resize(Size {
            width: 100,
            height: 30,
        }));

        assert_eq!(outcome, CommandBarOutcome::Stay);
        assert_eq!(bar.get_caret_position().row, 29);
    }

    #[test]
    fn handle_command_quit_dismisses_the_prompt() {
        let mut bar = bar_with_text("file.txt", 80);

        assert_eq!(
            bar.handle_command(EditorCommand::Quit),
            CommandBarOutcome::Cancelled
        );
    }

    #[test]
    fn handle_command_save_leaves_the_prompt_open() {
        // Only Enter confirms a prompt: Save is meaningless to the bar
        // itself, and the caller decides whether to act on it.
        let mut bar = bar_with_text("file.txt", 80);

        let outcome = bar.handle_command(EditorCommand::Save);

        assert_eq!(outcome, CommandBarOutcome::Stay);
        assert_eq!(bar.get_text(), "file.txt");
    }

    #[test]
    fn handle_command_find_leaves_the_prompt_open() {
        // Like Save, Find is meaningless to the bar itself: the caller
        // decides whether to act on the open prompt.
        let mut bar = bar_with_text("file.txt", 80);

        let outcome = bar.handle_command(EditorCommand::Find);

        assert_eq!(outcome, CommandBarOutcome::Stay);
        assert_eq!(bar.get_text(), "file.txt");
    }
}
