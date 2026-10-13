use crate::common::display::fit_to_width;
use crate::common::layout::{BarLayout, STATUS_BAR_OFFSET};
use crate::terminal::{Size, Terminal};
use unicode_width::UnicodeWidthStr;

/// Shown in place of a file name the document does not have yet.
const NO_FILE: &str = "[No file]";
/// The maximum number of display columns a file name may occupy.
const FILE_NAME_WIDTH: usize = 50;

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
    fn as_line(&self, width: usize) -> String {
        let file = fit_to_width(
            self.file_name.as_deref().unwrap_or(NO_FILE),
            FILE_NAME_WIDTH,
        );
        let state = if self.is_modified {
            "modified"
        } else {
            "unmodified"
        };
        let line_frac = format!(
            "{}/{}",
            self.current_line.saturating_add(1),
            self.total_lines
        );
        let middle = format!(" - {} lines | ({state})", self.total_lines);
        let align_width = width
            .saturating_sub(middle.width())
            .saturating_sub(file.width());
        format!("{file}{middle}{line_frac:>align_width$}")
    }
}

/// Renders a single line of document information at the bottom of the terminal.
pub struct StatusBar {
    state: DocumentStatus,
    needs_redraw: bool,
    layout: BarLayout,
}

impl StatusBar {
    pub fn new(size: Size) -> Self {
        Self {
            state: DocumentStatus::default(),
            needs_redraw: true,
            layout: BarLayout::new(size, STATUS_BAR_OFFSET),
        }
    }

    pub fn resize(&mut self, size: Size) {
        self.layout.resize(size);
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

        let row = self.layout.row();
        let _ = Terminal::clear_row(row);
        let text = fit_to_width(
            &self.state.as_line(self.layout.width()),
            self.layout.width(),
        );
        let result = Terminal::print_inverted_row(row, &text);
        debug_assert!(result.is_ok(), "Failed to render status bar");
        self.needs_redraw = false;
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
        assert_eq!(bar().layout.row(), 22);
    }

    #[test]
    fn resize_moves_the_bar_and_marks_it_dirty() {
        let mut bar = bar();
        bar.resize(Size {
            width: 100,
            height: 30,
        });
        assert_eq!(bar.layout.row(), 28);
        assert_eq!(bar.layout.width(), 100);
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
        let line = status(Some("file.txt"), 5, 2, true).as_line(80);
        assert!(line.contains("file.txt"));
        assert!(line.contains("5 lines"));
        assert!(line.contains("3/5"));
        assert!(line.contains("modified"));
    }

    #[test]
    fn rendered_line_without_file_shows_placeholder() {
        let line = status(None, 1, 0, false).as_line(80);
        assert!(line.contains("No file"));
        assert!(line.contains("1 line"));
        assert!(line.contains("1/1"));
        assert!(line.contains("unmodified"));
    }

    #[test]
    fn rendered_line_is_padded_to_the_width() {
        let line = status(Some("file.txt"), 5, 2, true).as_line(80);
        assert_eq!(line.width(), 80);
    }

    #[test]
    fn full_width_file_names_do_not_break_the_layout() {
        // Each CJK character occupies two columns, so the line must still
        // fit its width without truncating a character.
        let line = status(Some("中文文件.txt"), 5, 2, true).as_line(80);
        assert!(line.contains("中文文件.txt"));
        assert!(line.contains("3/5"));
        assert_eq!(line.width(), 80);
    }
}
