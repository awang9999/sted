use crate::common::constants::LONG_MESSAGE_DURATION;
use crate::common::display::fit_to_width;
use crate::common::layout::{BarLayout, MESSAGE_BAR_OFFSET};
use crate::terminal::{Size, Terminal};
use std::time::{Duration, Instant};

/// A message and how long it stays on the message bar before it expires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageState {
    pub message: String,
    pub duration: Duration,
    pub created_at: Instant,
}

impl MessageState {
    pub fn new(message: String, duration: Duration) -> Self {
        Self {
            message,
            duration,
            created_at: Instant::now(),
        }
    }

    /// Whether the message has outlived its display duration.
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.duration
    }

    fn as_line(&self, width: usize) -> String {
        fit_to_width(&self.message, width)
    }
}

impl Default for MessageState {
    fn default() -> Self {
        MessageState::new(String::new(), LONG_MESSAGE_DURATION)
    }
}

/// Renders a single line of editor message at the bottom of the terminal.
///
/// The bar owns the message state and its display timer:
/// [`MessageBar::set_message`] replaces the current message and
/// (re)starts the timer, and [`MessageBar::render`] drops the message
/// once it has expired.
pub struct MessageBar {
    state: MessageState,
    needs_redraw: bool,
    layout: BarLayout,
}

impl MessageBar {
    pub fn new(size: Size) -> Self {
        Self {
            state: MessageState::default(),
            needs_redraw: true,
            layout: BarLayout::new(size, MESSAGE_BAR_OFFSET),
        }
    }

    /// Replaces the message shown on the bar and (re)starts its display
    /// timer, so it stays visible for `duration`.
    pub fn set_message(&mut self, message: String, duration: Duration) {
        self.state = MessageState::new(message, duration);
        self.needs_redraw = true;
    }

    /// The message the bar currently holds, or the empty default state
    /// once a render dropped the previous message as expired.
    // Reached through `View::get_message`, which only the test suite uses.
    #[allow(dead_code)]
    pub fn message(&self) -> &MessageState {
        &self.state
    }

    pub fn resize(&mut self, size: Size) {
        self.layout.resize(size);
        self.needs_redraw = true;
    }

    pub fn render(&mut self) {
        if self.state.is_expired() {
            self.state = MessageState::default();
            self.needs_redraw = true;
        }

        if !self.needs_redraw {
            return;
        }

        let row = self.layout.row();
        let _ = Terminal::clear_row(row);
        let text = self.state.as_line(self.layout.width());
        let result = Terminal::print_row(row, &text);
        debug_assert!(result.is_ok(), "Failed to render message bar");
        self.needs_redraw = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::constants::DEFAULT_MESSAGE_DURATION;

    fn bar() -> MessageBar {
        MessageBar::new(Size {
            width: 80,
            height: 24,
        })
    }

    fn state(message: &str) -> MessageState {
        MessageState::new(message.to_string(), DEFAULT_MESSAGE_DURATION)
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
        assert_eq!(state.duration, LONG_MESSAGE_DURATION);
    }

    #[test]
    fn bar_sits_on_the_last_reserved_row() {
        assert_eq!(bar().layout.row(), 23);
    }

    #[test]
    fn bar_row_saturates_when_terminal_is_shorter_than_reserved_rows() {
        let bar = MessageBar::new(Size {
            width: 80,
            height: 0,
        });
        assert_eq!(bar.layout.row(), 1);
    }

    #[test]
    fn resize_moves_the_bar_and_marks_it_dirty() {
        let mut bar = bar();
        bar.resize(Size {
            width: 100,
            height: 30,
        });
        assert_eq!(bar.layout.row(), 29);
        assert_eq!(bar.layout.width(), 100);
        assert!(bar.needs_redraw);
    }

    #[test]
    fn set_message_marks_the_bar_dirty() {
        let mut bar = bar();
        bar.render();
        assert!(!bar.needs_redraw);
        bar.set_message(String::from("hello"), DEFAULT_MESSAGE_DURATION);
        assert!(bar.needs_redraw);
        bar.render();
        assert!(!bar.needs_redraw);
    }

    #[test]
    fn set_message_replaces_the_stored_message() {
        let mut bar = bar();
        bar.set_message(String::from("hello"), DEFAULT_MESSAGE_DURATION);
        bar.render();
        bar.set_message(String::from("goodbye"), DEFAULT_MESSAGE_DURATION);
        assert!(bar.needs_redraw);
        assert_eq!(bar.message().message, "goodbye");
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
        bar.set_message(String::from("hello"), Duration::from_secs(5));
        // Backdate the message so it is already past its duration.
        bar.state.created_at = bar
            .state
            .created_at
            .checked_sub(bar.state.duration)
            .expect("clock is far enough past the epoch to backdate 5s");
        bar.render();
        assert_eq!(bar.state.message, "");
    }

    #[test]
    fn unexpired_message_survives_render() {
        let mut bar = bar();
        bar.set_message(String::from("hello"), Duration::from_secs(5));
        bar.render();
        assert_eq!(bar.state.message, "hello");
    }

    #[test]
    fn is_expired_reports_fresh_messages_as_not_expired() {
        assert!(!state("hello").is_expired());
    }
}
