use crate::common::constants::{
    DEFAULT_MESSAGE_DURATION, LONG_MESSAGE_DURATION, RESERVED_BOTTOM_LINES,
};
use crate::terminal::{Size, Terminal};
use std::time::{Duration, Instant};

// The information the message bar displays about the current view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageState {
    pub message: String,
    pub duration: Duration,
    pub created_at: Instant,
}

impl Default for MessageState {
    fn default() -> Self {
        MessageState::create_message(String::new(), LONG_MESSAGE_DURATION)
    }
}

impl MessageState {
    pub fn create_default_duration(message: String) -> Self {
        MessageState::create_message(message, DEFAULT_MESSAGE_DURATION)
    }

    pub fn create_message(message: String, duration_sec: u64) -> Self {
        Self {
            message,
            duration: Duration::new(duration_sec, 0),
            created_at: Instant::now(),
        }
    }

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
        if self.state.created_at.elapsed() > self.state.duration {
            self.update(MessageState::default());
        }

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
        MessageState::create_default_duration(message.to_string())
    }

    #[test]
    fn new_bar_is_ready_to_render() {
        assert!(bar().needs_redraw);
    }

    #[test]
    fn new_bar_starts_with_empty_message() {
        // Compare fields rather than whole states: each MessageState carries
        // its own created_at Instant, so two freshly built states are never
        // equal even when their content matches.
        let state = bar().state;
        assert_eq!(state.message, "");
        assert_eq!(state.duration, Duration::from_secs(LONG_MESSAGE_DURATION));
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
        let message = state("hello");
        bar.update(message.clone());
        bar.render();
        assert!(!bar.needs_redraw);
        bar.update(message);
        assert!(!bar.needs_redraw);
    }

    #[test]
    fn update_with_same_text_but_fresh_state_marks_dirty() {
        // A freshly built state (even with identical text) starts a new
        // display timer, so it must be treated as a change.
        let mut bar = bar();
        bar.update(state("hello"));
        bar.render();
        bar.update(state("hello"));
        assert!(bar.needs_redraw);
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

    #[test]
    fn expired_message_is_cleared_on_render() {
        let mut bar = bar();
        let mut state = MessageState::create_message(String::from("hello"), 5);
        // Backdate the message so it is already past its duration.
        state.created_at = state
            .created_at
            .checked_sub(state.duration)
            .expect("clock is far enough past the epoch to backdate 5s");
        bar.update(state);
        bar.render();
        assert_eq!(bar.state.message, "");
    }

    #[test]
    fn unexpired_message_survives_render() {
        let mut bar = bar();
        bar.update(MessageState::create_message(String::from("hello"), 5));
        bar.render();
        assert_eq!(bar.state.message, "hello");
    }
}
