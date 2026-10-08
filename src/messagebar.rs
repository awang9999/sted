use crate::common::constants::RESERVED_BOTTOM_LINES;
use crate::terminal::{Size, Terminal};

// The information the message bar displays about the current view.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MessageState {
    pub message: String,
}

impl MessageState {
    fn as_line(&self, width: usize) -> String {
        format!("{:<.width$}", self.message)
    }
}

/// Renders a single line of editor message at the bottom of the terminal.
pub struct MessageBar {
    state: MessageState,
    needs_redraw: bool,
    width: usize,
    row: usize,
}

impl MessageBar {
    pub fn new(size: Size) -> Self {
        Self {
            state: MessageState::default(),
            needs_redraw: true,
            width: size.width,
            row: Self::bar_row(size),
        }
    }

    /// The row the bar is rendered on: the first of the rows reserved at the
    /// bottom of the terminal.
    fn bar_row(terminal: Size) -> usize {
        terminal
            .height
            .saturating_sub(RESERVED_BOTTOM_LINES)
            .saturating_add(1)
    }

    pub fn resize(&mut self, size: Size) {
        self.width = size.width;
        self.row = Self::bar_row(size);
        self.needs_redraw = true;
    }

    pub fn update(&mut self, state: MessageState) {
        if state != self.state {
            self.state = state;
            self.needs_redraw = true;
        }
    }

    pub fn render(&mut self) {
        if !self.needs_redraw {
            return;
        }

        let _ = Terminal::clear_row(self.row);
        let mut text = self.state.as_line(self.width);
        text.truncate(self.width);
        let result = Terminal::print_row(self.row, &text);
        debug_assert!(result.is_ok(), "Failed to render status bar");
        self.needs_redraw = false;
    }
}

impl Default for MessageBar {
    fn default() -> Self {
        Self::new(Terminal::size().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar() -> MessageBar {
        MessageBar::new(Size {
            width: 80,
            height: 24,
        })
    }

    fn state(message: &str) -> MessageState {
        MessageState {
            message: message.to_string(),
        }
    }

    #[test]
    fn new_bar_is_ready_to_render() {
        assert!(bar().needs_redraw);
    }

    #[test]
    fn new_bar_starts_with_empty_message() {
        assert_eq!(bar().state, MessageState::default());
    }

    #[test]
    fn bar_sits_on_the_last_reserved_row() {
        assert_eq!(bar().row, 23);
    }

    #[test]
    fn bar_row_saturates_when_terminal_is_shorter_than_reserved_rows() {
        let bar = MessageBar::new(Size {
            width: 80,
            height: 0,
        });
        assert_eq!(bar.row, 1);
    }

    #[test]
    fn resize_moves_the_bar_and_marks_it_dirty() {
        let mut bar = bar();
        bar.resize(Size {
            width: 100,
            height: 30,
        });
        assert_eq!(bar.row, 29);
        assert_eq!(bar.width, 100);
        assert!(bar.needs_redraw);
    }

    #[test]
    fn update_with_identical_state_does_not_mark_dirty() {
        let mut bar = bar();
        bar.update(state("hello"));
        bar.render();
        assert!(!bar.needs_redraw);
        bar.update(state("hello"));
        assert!(!bar.needs_redraw);
    }

    #[test]
    fn update_with_changed_state_marks_dirty() {
        let mut bar = bar();
        bar.update(state("hello"));
        bar.render();
        bar.update(state("goodbye"));
        assert!(bar.needs_redraw);
    }

    #[test]
    fn rendered_line_keeps_short_message_untouched() {
        let line = state("hello").as_line(80);
        assert_eq!(line, "hello");
    }

    #[test]
    fn rendered_line_is_truncated_to_width_when_message_is_longer() {
        let line = state("01234567890123456789").as_line(10);
        assert_eq!(line, "0123456789");
    }

    #[test]
    fn rendered_line_truncates_by_character_not_byte() {
        let line = state("café").as_line(3);
        assert_eq!(line, "caf");
    }

    #[test]
    fn rendered_line_is_empty_when_message_is_empty() {
        let line = state("").as_line(10);
        assert_eq!(line, "");
    }
}
