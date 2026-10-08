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
