use crate::common::constants::RESERVED_BOTTOM_LINES;
use crate::terminal::{Size, Terminal};

/// The information the status bar displays about the current document.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DocumentStatus {
    pub file_name: Option<String>,
    pub total_lines: usize,
    /// Zero-based index of the line the caret is currently on.
    pub current_line: usize,
    pub is_modified: bool,
}

impl DocumentStatus {
    fn as_line(&self) -> String {
        let file = self.file_name.as_deref().unwrap_or("No file");
        let state = if self.is_modified {
            "modified"
        } else {
            "unmodified"
        };
        format!(
            "{file} | {} lines | line {} | {state}",
            self.total_lines,
            self.current_line.saturating_add(1)
        )
    }
}

/// Renders a single line of document information at the bottom of the terminal.
pub struct StatusBar {
    state: DocumentStatus,
    needs_redraw: bool,
    width: usize,
    row: usize,
}

impl StatusBar {
    pub fn new(size: Size) -> Self {
        Self {
            state: DocumentStatus::default(),
            needs_redraw: true,
            width: size.width,
            row: Self::bar_row(size),
        }
    }

    /// The row the bar is rendered on: the first of the rows reserved at the
    /// bottom of the terminal.
    fn bar_row(terminal: Size) -> usize {
        terminal.height.saturating_sub(RESERVED_BOTTOM_LINES)
    }

    pub fn resize(&mut self, size: Size) {
        self.width = size.width;
        self.row = Self::bar_row(size);
        self.needs_redraw = true;
    }

    pub fn update(&mut self, state: DocumentStatus) {
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
        let mut text = self.state.as_line();
        text.truncate(self.width);
        let result = Terminal::print_row(self.row, &text);
        debug_assert!(result.is_ok(), "Failed to render status bar");
        self.needs_redraw = false;
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new(Terminal::size().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar() -> StatusBar {
        StatusBar::new(Size {
            width: 80,
            height: 24,
        })
    }

    fn status(file: Option<&str>, lines: usize, line: usize, modified: bool) -> DocumentStatus {
        DocumentStatus {
            file_name: file.map(str::to_string),
            total_lines: lines,
            current_line: line,
            is_modified: modified,
        }
    }

    #[test]
    fn new_bar_is_ready_to_render() {
        assert!(bar().needs_redraw);
    }

    #[test]
    fn bar_sits_on_the_first_reserved_row() {
        assert_eq!(bar().row, 22);
    }

    #[test]
    fn resize_moves_the_bar_and_marks_it_dirty() {
        let mut bar = bar();
        bar.resize(Size {
            width: 100,
            height: 30,
        });
        assert_eq!(bar.row, 28);
        assert_eq!(bar.width, 100);
        assert!(bar.needs_redraw);
    }

    #[test]
    fn update_with_identical_state_does_not_mark_dirty() {
        let mut bar = bar();
        bar.render();
        assert!(!bar.needs_redraw);
        bar.update(status(Some("file.txt"), 5, 2, true));
        bar.render();
        bar.update(status(Some("file.txt"), 5, 2, true));
        assert!(!bar.needs_redraw);
    }

    #[test]
    fn update_with_changed_state_marks_dirty() {
        let mut bar = bar();
        bar.render();
        bar.update(status(Some("file.txt"), 5, 2, true));
        bar.render();
        bar.update(status(Some("file.txt"), 6, 2, true));
        assert!(bar.needs_redraw);
    }

    #[test]
    fn rendered_line_contains_all_status_information() {
        let line = status(Some("file.txt"), 5, 2, true).as_line();
        assert!(line.contains("file.txt"));
        assert!(line.contains("5 lines"));
        assert!(line.contains("line 3"));
        assert!(line.contains("modified"));
    }

    #[test]
    fn rendered_line_without_file_shows_placeholder() {
        let line = status(None, 1, 0, false).as_line();
        assert!(line.contains("No file"));
        assert!(line.contains("1 line"));
        assert!(line.contains("line 1"));
        assert!(line.contains("unmodified"));
    }
}
