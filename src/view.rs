use crate::buffer::Buffer;
use crate::common::constants::{
    DEFAULT_MESSAGE_DURATION, LONG_MESSAGE_DURATION, RESERVED_BOTTOM_LINES,
};
use crate::common::line::Line;
use crate::common::types::Location;
use crate::editorcommand::{Direction, EditorCommand};
use crate::messagebar::{MessageBar, MessageState};
use crate::statusbar::{DocumentStatus, StatusBar};
use crate::terminal::{Position, Size, Terminal};
use std::cmp::min;

/// Shown on the message bar until a more specific message replaces it.
const DEFAULT_MESSAGE: &str = "HELP: C-s = Save | C-q = quit";

pub struct View {
    buffer: Buffer,
    modified: bool,
    size: Size,
    pub location: Location,      // location in the text (col, row) of grapheme
    pub scroll_offset: Position, // top left corner of viewport as a grid (independent of grapheme width)
    desired_x: usize,
    status_bar: StatusBar,
    message_bar: MessageBar,
    message: MessageState,
    /// Set when a quit request was refused because of unsaved changes; a
    /// second consecutive Ctrl-Q then confirms the quit.
    quit_pending: bool,
}

impl Default for View {
    fn default() -> Self {
        Self::new(Terminal::size().unwrap_or_default())
    }
}

impl View {
    pub fn new(size: Size) -> Self {
        View {
            buffer: Buffer::default(),
            modified: true,
            size: Self::viewport_size(size),
            location: Location::default(),
            scroll_offset: Position::default(),
            desired_x: 0,
            status_bar: StatusBar::new(size),
            message_bar: MessageBar::new(size),
            message: MessageState::create_default_duration(String::from(DEFAULT_MESSAGE)),
            quit_pending: false,
        }
    }

    /// The part of the terminal the buffer is rendered into: the full
    /// terminal minus the rows reserved for the status bar at the bottom.
    fn viewport_size(terminal: Size) -> Size {
        Size {
            width: terminal.width,
            height: terminal.height.saturating_sub(RESERVED_BOTTOM_LINES),
        }
    }

    pub fn get_status(&self) -> DocumentStatus {
        DocumentStatus {
            file_name: self.buffer.file_name.clone(),
            total_lines: self.buffer.lines.len(),
            current_line: self.location.y,
            is_modified: self.buffer.modified,
        }
    }

    /// Whether the buffer is associated with a file on disk. A buffer
    /// without a file name is a new, unsaved document.
    pub fn has_file_name(&self) -> bool {
        self.buffer.file_name.is_some()
    }

    /// The view's current message, stored so its display timer survives
    /// across renders. Returning a clone (not a fresh state) is what keeps
    /// the message from being re-armed on every render pass.
    pub fn get_message(&self) -> MessageState {
        self.message.clone()
    }

    /// Replaces the message shown on the message bar and (re)starts its
    /// display timer, so it stays visible for `duration_sec` seconds.
    pub fn set_message(&mut self, message: String, duration_sec: u64) {
        self.message = MessageState::create_message(message, duration_sec);
    }

    pub fn resize(&mut self, to: Size) {
        // When the terminal shrinks, every row at or above the new height is
        // gone, but the new last row keeps whatever it held before (buffer
        // text or the status bar). Clear it so no stale content lingers below
        // the viewport, since neither render pass touches that row.
        let old_terminal_height = self.size.height.saturating_add(RESERVED_BOTTOM_LINES);
        if to.height > 0 && to.height < old_terminal_height {
            let _ = Terminal::clear_row(to.height.saturating_sub(1));
        }
        self.size = Self::viewport_size(to);
        self.modified = true;
        self.status_bar.resize(to);
        self.message_bar.resize(to);
    }

    pub fn load(&mut self, file_name: &str) {
        match Buffer::load(file_name) {
            Ok(buffer) => self.buffer = buffer,
            Err(error) => {
                // The file could not be opened: fall back to an empty buffer
                // (so nothing fabricated can ever be saved to disk) and keep
                // the error visible until the user has read it.
                self.buffer = Buffer::default();
                self.set_message(error, LONG_MESSAGE_DURATION);
            }
        }
        self.modified = true;
    }

    pub fn get_caret_position(&self) -> Position {
        let caret_position = Position {
            col: self.buffer.get_col_from_text_location(self.location),
            row: self.location.y,
        };
        caret_position.subtract(&self.scroll_offset)
    }

    pub fn handle_command(&mut self, event: EditorCommand) {
        // Any other activity supersedes a pending quit warning, so the user
        // must press Ctrl-Q twice again to quit after it.
        self.quit_pending = false;
        match event {
            EditorCommand::Resize(size) => self.resize(size),
            EditorCommand::Move(direction) => self.move_location(direction),
            EditorCommand::Insert(c) => self.handle_insert(c),
            EditorCommand::Enter => self.handle_newline(),
            EditorCommand::Delete => self.handle_delete(),
            EditorCommand::BackSpace => self.handle_backspace(),
            EditorCommand::Save => self.handle_save(),
            // Quit is handled by the editor. Escape is only meaningful to
            // the command bar; the view ignores both.
            EditorCommand::Quit | EditorCommand::Escape => (),
        }
    }

    /// Handles a quit request and reports whether the editor should exit.
    ///
    /// When the buffer has unsaved changes, the first request is refused and
    /// a warning is shown on the message bar; a second consecutive request
    /// confirms the quit. Any other command in between (see
    /// `handle_command`) resets the warning.
    pub fn handle_quit(&mut self) -> bool {
        if self.buffer.modified && !self.quit_pending {
            self.quit_pending = true;
            let file_name = self.buffer.file_name.as_deref().unwrap_or("(unnamed)");
            self.set_message(
                format!("WARNING! File {file_name} has unsaved changes. Press Ctrl-Q again to quit without saving"),
                LONG_MESSAGE_DURATION,
            );
            return false;
        }
        true
    }

    pub fn render_buffer(&mut self) {
        let Size { height, width } = self.size;
        if height == 0 || width == 0 {
            return;
        }

        for current_row in 0..height {
            let _ = Terminal::clear_row(current_row);
            let top = self.scroll_offset.row;

            if let Some(line) = self.buffer.lines.get(current_row.saturating_add(top)) {
                let left = self.scroll_offset.col;
                let right = self.scroll_offset.col.saturating_add(width);
                let truncated_line = &line.get(left..right);
                Self::render_line(current_row, truncated_line);
            } else {
                Self::render_line(current_row, "~");
            }

            if current_row.saturating_add(1) < height {
                let _ = Terminal::move_cursor_to(0, current_row.saturating_add(1));
            }
        }
    }

    fn render_line(row: usize, line_text: &str) {
        let result = Terminal::print_row(row, line_text);
        debug_assert!(result.is_ok(), "Failed to render line");
    }

    /// Draws everything the view owns: the buffer viewport and the status
    /// bar. The height guards use the live terminal size because the stored
    /// `size` is the viewport size, which cannot distinguish terminal
    /// heights below the reserved bottom rows.
    pub fn render(&mut self) {
        let terminal_height = Terminal::size().unwrap().height;

        // The buffer needs one viewport row, i.e. three terminal rows once
        // the reserved bottom rows are included.
        if terminal_height >= 3 && self.modified {
            self.render_buffer();

            if self.buffer.is_empty() {
                let _ = self.render_welcome_screen();
            }

            self.modified = false;
        }

        // The bar always draws from the view's current state, so it can
        // never lag behind the buffer or the caret.
        if terminal_height >= 2 {
            self.status_bar.update(self.get_status());
            self.status_bar.render();
        }

        if terminal_height >= 1 {
            // Drop messages that have outlived their duration before pushing
            // them, so the bar can't re-animate a message it already cleared.
            if self.message.created_at.elapsed() >= self.message.duration {
                self.message = MessageState::default();
            }

            self.message_bar.update(self.get_message());
            self.message_bar.render();
        }
    }

    fn render_welcome_screen(&self) -> Result<(), std::io::Error> {
        let welcome = "Welcome to STED!";
        let sted = "The (S)imple (T)erminal (Ed)itor";
        let version = "Version 1.0.0";

        #[allow(clippy::integer_division)]
        let welcome_row = (self.size.height / 2).saturating_sub(2);
        // we allow this since we don't care if our welcome message is put _exactly_ in the middle.
        // it's allowed to be a bit to the left or right.
        #[allow(clippy::integer_division)]
        let welcome_col = self.size.width.saturating_sub(welcome.len()) / 2;
        let sted_row = welcome_row.saturating_add(1);
        #[allow(clippy::integer_division)]
        let sted_col = self.size.width.saturating_sub(sted.len()) / 2;
        let version_row = welcome_row.saturating_add(2);
        #[allow(clippy::integer_division)]
        let version_col = self.size.width.saturating_sub(version.len()) / 2;

        Terminal::print_at(welcome_col.saturating_sub(1), welcome_row, welcome)?;

        Terminal::print_at(sted_col.saturating_sub(1), sted_row, sted)?;

        Terminal::print_at(version_col.saturating_sub(1), version_row, version)?;

        Ok(())
    }

    pub fn move_location(&mut self, direction: Direction) {
        let Location { mut x, mut y } = self.location;
        let height = self.size.height;
        let buffer_length = self.buffer.lines.len();

        match direction {
            // Vertical movements
            Direction::Up => y = y.saturating_sub(1),
            Direction::Down => {
                if y < buffer_length {
                    y = y.saturating_add(1).min(buffer_length);
                }
            }
            Direction::PageUp => y = y.saturating_sub(height),
            Direction::PageDown => {
                let page_y = y.saturating_add(height);
                if page_y > buffer_length || self.buffer.lines.is_empty() {
                    y = buffer_length;
                } else {
                    y = page_y.min(buffer_length);
                }
            }

            // Horizontal movements
            Direction::Left => {
                if x == 0 && y > 0 {
                    y = y.saturating_sub(1);
                    x = self.buffer.lines.get(y).map_or(0, Line::grapheme_count);
                } else {
                    x = x.saturating_sub(1);
                }
                self.desired_x = x;
            }

            Direction::Right => {
                if let Some(len) = self.buffer.lines.get(y).map(Line::grapheme_count) {
                    if x >= len && y < buffer_length {
                        y = y.saturating_add(1);
                        x = 0;
                    } else if x < len {
                        x = x.saturating_add(1);
                    }
                }
                self.desired_x = x;
            }

            // Home / End
            Direction::Home => {
                x = 0;
                self.desired_x = x;
            }
            Direction::End => {
                if let Some(len) = self.buffer.lines.get(y).map(Line::grapheme_count) {
                    x = len;
                    self.desired_x = x;
                }
            }
        }

        // Clamp x column to fit within the new line's length when moving vertically
        if matches!(
            direction,
            Direction::Up | Direction::Down | Direction::PageUp | Direction::PageDown
        ) {
            x = self
                .buffer
                .lines
                .get(y)
                .map_or(0, |l| min(self.desired_x, l.grapheme_count()));
        }

        self.set_location(x, y);
        self.scroll_location_into_view();
    }

    pub fn scroll_location_into_view(&mut self) {
        let (x, y) = (self.location.x, self.location.y);
        let mut scroll = self.scroll_offset;
        let Size { width, height } = self.size;
        let prev = self.scroll_offset;

        // Vertical scroll
        // If cursor y is above the viewport, snap scroll_y to the cursor
        if y < scroll.row {
            scroll.row = y;
        }
        // If cursor y is below the viewport, push the viewport down by the offset
        else if y >= scroll.row.saturating_add(height) {
            scroll.row = y.saturating_sub(height).saturating_add(1);
        }

        // Horizontal scroll - when cursor is past EOF, horizontal scroll data doesn't exist yet
        let clamped_y = y.min(self.buffer.lines.len().saturating_sub(1));
        let line = if self.buffer.lines.is_empty() {
            None
        } else {
            self.buffer
                .lines
                .get(clamped_y)
                .or_else(|| self.buffer.lines.last())
        };

        if let Some(line) = line {
            // Use the target line's saved horizontal offset as the baseline
            let line_scroll_x = line.scroll_x;
            let line_graphemes = line.grapheme_count();
            let line_len = line.width_until(line_graphemes);
            let cur_x_position = line.width_until(x);

            // If cursor x is left of the baseline, scroll left to the cursor
            if cur_x_position < line_scroll_x {
                scroll.col = cur_x_position;
            }
            // If cursor x is right of the viewport, scroll right to keep it visible
            else if cur_x_position >= line_scroll_x.saturating_add(width) {
                scroll.col = cur_x_position.saturating_sub(width).saturating_add(1);
            // Use the saved line scroll to keep the cursor relative to the last time the
            // user was on this line.
            } else {
                // Only if x is in the viewport of line_scroll_x but not within the viewport of scroll.x
                // This prevents jumpy experience for a lot of tiny skips while moving vertically
                scroll.col = line_scroll_x;
            }

            // Clamp scroll_x to the line's length so we never scroll past the visible content
            let max_scroll_x = line_len.saturating_sub(width);
            scroll.col = scroll.col.min(max_scroll_x);

            // Persist the updated horizontal scroll state to the target line
            if let Some(line) = self.buffer.lines.get_mut(clamped_y) {
                line.scroll_x = scroll.col;
            }
        }

        // Apply the new scroll offset and mark dirty if the view changed
        self.scroll_offset = scroll;
        self.modified = scroll.col != prev.col || scroll.row != prev.row;
    }

    pub fn set_location(&mut self, x: usize, y: usize) {
        self.set_location_coord(Location { x, y });
    }

    pub fn set_location_coord(&mut self, pos: Location) {
        self.location = pos;
    }

    /// Inserts a character, moves the caret right, scrolls it into view, marks buffer and view as modified.
    fn handle_insert(&mut self, c: char) {
        self.buffer.insert_char(self.location, c);
        self.set_location(self.location.x.saturating_add(1), self.location.y);
        self.scroll_location_into_view();
        self.modified = true;
    }

    /// Splits the line at the caret (or appends an empty line when the caret
    /// is below the last line) and moves the caret to the beginning of the
    /// new line. Forces a re-render.
    fn handle_newline(&mut self) {
        let new_location = self.buffer.newline(self.location);
        self.set_location(new_location.x, new_location.y);
        self.desired_x = 0;
        self.scroll_location_into_view();
        self.modified = true;
    }

    /// Deletes the grapheme cluster at the caret position and marks the view as modified.
    fn handle_delete(&mut self) {
        self.buffer.delete(self.location);
        self.modified = true;
    }

    fn handle_backspace(&mut self) {
        let Location { x, y } = self.location;

        // No-op at the very start of the buffer.
        if x == 0 && y == 0 {
            return;
        }

        // Move the caret one grapheme left. At x == 0 this wraps to the end of the
        // previous line, so the two lines must be joined; otherwise the caret just
        // stepped back and we delete the grapheme now to its left.
        self.move_location(Direction::Left);

        if x == 0 {
            self.buffer.consolidate_lines(y, y.saturating_sub(1));
        } else {
            self.handle_delete();
        }

        // guarantees a re-render.
        self.modified = true;
    }

    /// Associates the buffer with `file_name` and saves it to that file,
    /// reporting the outcome on the message bar. Used when the user named a
    /// previously unnamed document through the command bar.
    pub fn save_as(&mut self, file_name: &str) {
        self.buffer.file_name = Some(file_name.to_string());
        self.handle_save();
    }

    /// Writes the buffer to its file and reports the outcome on the message
    /// bar: a brief success note naming the file, or an error that stays
    /// visible until the user has read it.
    fn handle_save(&mut self) {
        match self.buffer.save() {
            Ok(()) => {
                // Briefly confirm the save on the message bar.
                let file_name = self.buffer.file_name.as_deref().unwrap_or_default();
                self.set_message(
                    format!("File saved successfully to {file_name}"),
                    DEFAULT_MESSAGE_DURATION,
                );
            }
            Err(_) => {
                // Keep the error on the bar until the user has read it:
                // it stays until a new message replaces it.
                self.set_message(String::from("Error writing file!"), LONG_MESSAGE_DURATION);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::constants::{LONG_MESSAGE_DURATION, TAB_WIDTH_SPACES};
    use crate::common::line::Line;
    use std::time::Duration;

    fn make_view(lines_data: &[&str]) -> View {
        let buffer = Buffer {
            lines: lines_data.iter().copied().map(Line::from).collect(),
            modified: true,
            file_name: None,
        };
        let terminal_size = Terminal::size().unwrap_or(Size {
            height: 24,
            width: 80,
        });
        View {
            buffer,
            modified: false,
            size: terminal_size,
            location: Location::default(),
            scroll_offset: Position::default(),
            quit_pending: false,
            desired_x: 0,
            status_bar: StatusBar::new(terminal_size),
            message_bar: MessageBar::new(terminal_size),
            message: MessageState::default(),
        }
    }

    #[test]
    fn get_message_returns_a_stable_state_across_calls() {
        let mut view = make_view(&[]);
        view.set_message(String::from("hi"), 5);
        assert_eq!(view.get_message(), view.get_message());
    }

    #[test]
    fn set_message_stores_message_and_duration() {
        let mut view = make_view(&[]);
        view.set_message(String::from("Saved!"), 5);
        let message = view.get_message();
        assert_eq!(message.message, "Saved!");
        assert_eq!(message.duration, Duration::from_secs(5));
    }

    #[test]
    fn save_with_file_name_shows_success_message_with_file_name() {
        let path =
            std::env::temp_dir().join(format!("sted_view_save_test_{}.txt", std::process::id()));
        std::fs::write(&path, "hello\n").expect("create temp file");
        let file_path = path.to_string_lossy().to_string();

        let mut view = make_view(&["hello"]);
        view.buffer.file_name = Some(file_path.clone());

        view.handle_save();

        let message = view.get_message();
        assert_eq!(
            message.message,
            format!("File saved successfully to {file_path}")
        );
        assert_eq!(
            message.duration,
            Duration::from_secs(DEFAULT_MESSAGE_DURATION)
        );
        assert!(!view.buffer.modified);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_without_file_name_shows_error_message() {
        let mut view = make_view(&["hello"]);

        view.handle_save();

        let message = view.get_message();
        assert_eq!(message.message, "Error writing file!");
        assert_eq!(message.duration, Duration::from_secs(LONG_MESSAGE_DURATION));
        assert!(view.buffer.modified);
    }

    #[test]
    fn save_as_names_the_buffer_and_reports_success() {
        let path =
            std::env::temp_dir().join(format!("sted_view_save_as_{}.txt", std::process::id()));
        let file_path = path.to_string_lossy().to_string();

        let mut view = make_view(&["hello"]);
        view.save_as(&file_path);

        let message = view.get_message();
        assert_eq!(
            message.message,
            format!("File saved successfully to {file_path}")
        );
        assert_eq!(
            message.duration,
            Duration::from_secs(DEFAULT_MESSAGE_DURATION)
        );
        assert_eq!(view.buffer.file_name.as_deref(), Some(file_path.as_str()));
        assert!(!view.buffer.modified);
        assert!(path.exists());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_as_of_unwritable_path_keeps_the_name_and_reports_error() {
        let mut view = make_view(&["hello"]);

        view.save_as("/nonexistent_dir_sted/cannot_be_created.txt");

        assert_eq!(view.get_message().message, "Error writing file!");
        // The failed name is kept, so the next save retries the same path.
        assert_eq!(
            view.buffer.file_name.as_deref(),
            Some("/nonexistent_dir_sted/cannot_be_created.txt")
        );
        assert!(view.buffer.modified);
    }

    #[test]
    fn has_file_name_reflects_the_buffer() {
        let mut view = make_view(&["a"]);
        assert!(!view.has_file_name());
        view.buffer.file_name = Some(String::from("x.txt"));
        assert!(view.has_file_name());
    }

    #[test]
    fn escape_is_ignored_by_the_view() {
        let mut view = make_view(&["a"]);

        view.handle_command(EditorCommand::Escape);

        assert_eq!(view.get_message().message, "");
    }

    #[test]
    fn load_of_missing_file_reports_error_on_message_bar() {
        let path =
            std::env::temp_dir().join(format!("sted_view_load_missing_{}.txt", std::process::id()));
        let file_path = path.to_string_lossy().to_string();

        let mut view = make_view(&["existing content"]);

        view.load(&file_path);

        let message = view.get_message();
        assert_eq!(
            message.message,
            format!("Error: Could not open file: {file_path}")
        );
        assert_eq!(message.duration, Duration::from_secs(LONG_MESSAGE_DURATION));
        // The buffer falls back to empty, unsaved state.
        assert!(view.buffer.is_empty());
        assert!(view.buffer.file_name.is_none());
    }

    #[test]
    fn load_of_existing_file_replaces_buffer_and_keeps_name() {
        let path = std::env::temp_dir().join(format!("sted_view_load_{}.txt", std::process::id()));
        std::fs::write(&path, "one\ntwo\n").expect("create temp file");
        let file_path = path.to_string_lossy().to_string();

        let mut view = make_view(&["existing content"]);

        view.load(&file_path);

        let texts: Vec<String> = view.buffer.lines.iter().map(ToString::to_string).collect();
        assert_eq!(texts, vec!["one", "two"]);
        assert_eq!(view.buffer.file_name.as_deref(), Some(file_path.as_str()));
        // A successful load must not touch the message bar.
        assert_eq!(view.get_message().message, "");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn quit_without_unsaved_changes_exits_immediately() {
        let mut view = make_view(&["abc"]);
        view.buffer.modified = false;

        assert!(view.handle_quit());
        // No warning should have been raised.
        assert_eq!(view.get_message().message, "");
    }

    #[test]
    fn quit_with_unsaved_changes_warns_and_refuses_first_request() {
        let mut view = make_view(&["abc"]);
        view.buffer.file_name = Some(String::from("file.txt"));

        assert!(!view.handle_quit());

        let message = view.get_message();
        assert_eq!(
            message.message,
            "WARNING! File file.txt has unsaved changes. Press Ctrl-Q again to quit without saving"
        );
        assert_eq!(message.duration, Duration::from_secs(LONG_MESSAGE_DURATION));
    }

    #[test]
    fn quit_with_unsaved_changes_and_no_file_name_names_it_unnamed() {
        let mut view = make_view(&["abc"]);

        assert!(!view.handle_quit());
        assert_eq!(
            view.get_message().message,
            "WARNING! File (unnamed) has unsaved changes. Press Ctrl-Q again to quit without saving"
        );
    }

    #[test]
    fn second_consecutive_quit_request_exits() {
        let mut view = make_view(&["abc"]);

        assert!(!view.handle_quit());
        assert!(view.handle_quit());
    }

    #[test]
    fn other_command_resets_pending_quit_warning() {
        let mut view = make_view(&["abc"]);

        assert!(!view.handle_quit()); // first press: warn
        view.handle_command(EditorCommand::Insert('x')); // activity in between
        assert!(!view.handle_quit()); // must warn again
        assert!(view.handle_quit()); // and the next press confirms
    }

    #[test]
    fn saving_clears_unsaved_changes_and_allows_immediate_quit() {
        let path = std::env::temp_dir().join(format!("sted_view_quit_{}.txt", std::process::id()));
        std::fs::write(&path, "abc\n").expect("create temp file");
        let file_path = path.to_string_lossy().to_string();

        let mut view = make_view(&["abc"]);
        view.buffer.file_name = Some(file_path.clone());

        assert!(!view.handle_quit()); // first press: warn
        view.handle_save(); // user saves instead
        assert!(view.handle_quit()); // nothing left to warn about

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn resize_shrink_updates_viewport_and_marks_modified() {
        let mut view = make_view(&["a", "b"]);
        view.modified = false;

        view.resize(Size {
            width: 80,
            height: 20,
        });

        assert_eq!(
            view.size.height,
            20usize.saturating_sub(RESERVED_BOTTOM_LINES)
        );
        assert_eq!(view.size.width, 80);
        assert!(view.modified);
    }

    #[test]
    fn handle_delete_marks_view_as_modified() {
        let mut view = make_view(&["abc"]);
        assert!(!view.modified); // render clears modified

        view.handle_delete();
        assert!(view.modified);
    }

    #[test]
    fn handle_delete_removes_grapheme_from_current_line() {
        let mut view = make_view(&["hello"]);
        view.set_location(0, 0);

        view.handle_delete(); // deletes grapheme at index 0 ('h')
        assert_eq!(view.buffer.lines[0].grapheme_count(), 4);
        let text: String = view.buffer.lines[0].to_string();
        assert!(!text.contains('h'));
    }

    #[test]
    fn handle_delete_on_nonzero_location() {
        let mut view = make_view(&["abcdef"]);
        view.set_location(3, 0); // caret between 'd' and 'e'

        view.handle_delete(); // deletes grapheme at index 3 ('d')
        let text: String = view.buffer.lines[0].to_string();
        assert!(!text.contains('d'));
    }

    #[test]
    fn handle_delete_with_out_of_bounds_location() {
        let mut view = make_view(&["abc"]);
        // Location is (0, 0) which is within bounds — buffer.delete handles row-OOB internally
        view.handle_delete();
        assert!(view.modified);
        let text: String = view.buffer.lines[0].to_string();
        assert!(!text.contains('a'));
    }

    #[test]
    fn handle_delete_across_multiple_lines() {
        let mut view = make_view(&["first", "second"]);
        let first_len_before = view.buffer.lines[0].grapheme_count();
        view.set_location(1, 0); // delete from first line
        view.handle_delete();
        assert_eq!(view.buffer.lines[0].grapheme_count(), first_len_before - 1);

        let second_len_before = view.buffer.lines[1].grapheme_count();
        view.set_location(2, 1); // delete from second line
        view.handle_delete();
        assert_eq!(view.buffer.lines[1].grapheme_count(), second_len_before - 1);

        assert!(view.modified);
    }

    #[test]
    fn backspace_at_beginning_of_line_merges_with_previous() {
        let mut view = make_view(&["ab", "cd"]);
        view.set_location(0, 1);

        view.handle_backspace();

        assert_eq!(view.buffer.lines.len(), 1);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "abcd");
        // caret lands at the end of the previous line
        assert_eq!((view.location.x, view.location.y), (2, 0));
        assert!(view.modified);
    }

    #[test]
    fn backspace_at_first_line_starts_nothing() {
        let mut view = make_view(&["ab"]);
        view.set_location(0, 0);

        view.handle_backspace();

        assert_eq!(view.buffer.lines.len(), 1);
        assert_eq!(view.buffer.lines[0].to_string(), "ab");
    }

    #[test]
    fn backspace_with_empty_previous_line_keeps_content() {
        let mut view = make_view(&["", "cd"]);
        view.set_location(0, 1);

        view.handle_backspace();

        assert_eq!(view.buffer.lines.len(), 1);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "cd");
        // caret lands at the end of the (empty) previous line
        assert_eq!((view.location.x, view.location.y), (0, 0));
    }

    #[test]
    fn backspace_mid_line_deletes_left_grapheme() {
        let mut view = make_view(&["abcd"]);
        view.set_location(2, 0);

        view.handle_backspace();

        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "acd");
        assert_eq!((view.location.x, view.location.y), (1, 0));
    }

    #[test]
    fn handle_insert_marks_view_as_modified() {
        let mut view = make_view(&["hello"]);
        assert!(!view.modified); // render clears modified

        view.handle_insert('x');
        assert!(view.modified);
    }

    #[test]
    fn handle_insert_moves_caret_right_by_one_column() {
        let mut view = make_view(&["hello"]);
        view.set_location(2, 0);
        assert_eq!(view.location.x, 2);

        view.handle_insert('x');
        assert_eq!(view.location.x, 3);
        assert_eq!(view.location.y, 0); // row should not change
    }

    #[test]
    fn handle_insert_at_beginning_of_line() {
        let mut view = make_view(&["hello"]);
        view.set_location(0, 0);

        view.handle_insert('x');
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "xhello");
    }

    #[test]
    fn handle_insert_at_end_of_line() {
        let mut view = make_view(&["hello"]);
        let line_len = view.buffer.lines[0].grapheme_count();
        view.set_location(line_len, 0); // position at end of "hello"

        view.handle_insert('!');
        assert_eq!(view.location.x, line_len + 1);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "hello!");
    }

    #[test]
    fn handle_insert_in_middle_of_line() {
        let mut view = make_view(&["hello world"]);
        view.set_location(5, 0); // position at the space

        view.handle_insert('|');
        assert_eq!(view.location.x, 6);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "hello| world");
    }

    #[test]
    fn handle_insert_on_empty_line() {
        let mut view = make_view(&[""]);
        view.set_location(0, 0);

        view.handle_insert('a');
        assert_eq!(view.location.x, 1);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "a");
    }

    #[test]
    fn handle_insert_on_empty_lines() {
        let mut view = make_view(&["", ""]);
        view.set_location(0, 0);

        view.handle_insert('x');
        assert_eq!(view.location.x, 1);
        view.handle_insert('y');
        assert_eq!(view.location.x, 2);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "xy");
    }

    #[test]
    fn handle_insert_on_different_rows() {
        let mut view = make_view(&["abc", "def"]);

        // Insert on first line
        view.set_location(0, 0);
        view.handle_insert('A');
        assert_eq!(view.location.x, 1);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "Aabc");

        // Move to second line and insert
        view.set_location(1, 1); // after 'd'
        view.handle_insert('X');
        assert_eq!(view.location.x, 2);
        let text: String = view.buffer.lines[1].to_string();
        assert_eq!(text, "dXef");
    }

    #[test]
    fn handle_insert_does_not_affect_other_lines() {
        let mut view = make_view(&["first", "middle", "last"]);
        let first_len_before = view.buffer.lines[0].grapheme_count();
        let last_len_before = view.buffer.lines[2].grapheme_count();
        view.set_location(2, 1); // middle line ("middle" has 6 graphemes)

        view.handle_insert('x');

        // Only the middle line should change — its length goes from 6 to 7
        assert_eq!(view.buffer.lines[1].grapheme_count(), 7);
        assert_eq!(view.buffer.lines[0].grapheme_count(), first_len_before);
        assert_eq!(view.buffer.lines[2].grapheme_count(), last_len_before);
    }

    #[test]
    fn handle_insert_with_multicharacter_graphemes() {
        let mut view = make_view(&["café"]);
        // 'f' is at grapheme index 3, insert '!' after the é (index 4)
        view.set_location(4, 0);

        view.handle_insert('!');
        assert_eq!(view.location.x, 5);
        let text: String = view.buffer.lines[0].to_string();
        assert_eq!(text, "café!");
    }

    #[test]
    fn handle_insert_tab_inserts_a_tab_grapheme() {
        let mut view = make_view(&["ab"]);
        view.set_location(1, 0);

        view.handle_insert('\t');

        let line = &view.buffer.lines[0];
        assert_eq!(line.grapheme_count(), 3);
        let text: String = line.to_string();
        assert_eq!(text, "a\tb");
        assert_eq!((view.location.x, view.location.y), (2, 0));
    }

    #[test]
    fn handle_insert_tab_on_blank_line_moves_caret_by_tab_width_columns() {
        let mut view = make_view(&[""]);
        view.set_location(0, 0);

        view.handle_insert('\t');

        // The caret is one grapheme in, but that grapheme (a tab) renders as
        // TAB_WIDTH_SPACES columns, so the on-screen caret column must match.
        assert_eq!(view.get_caret_position().col, TAB_WIDTH_SPACES);
        assert_eq!(view.location.y, 0);
    }

    #[test]
    fn handle_newline_splits_line_at_caret_and_moves_caret_to_new_line() {
        let mut view = make_view(&["abcdef"]);
        view.set_location(3, 0);

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 2);
        let top: String = view.buffer.lines[0].to_string();
        let bottom: String = view.buffer.lines[1].to_string();
        assert_eq!(top, "abc");
        assert_eq!(bottom, "def");
        assert_eq!((view.location.x, view.location.y), (0, 1));
        assert!(view.modified);
    }

    #[test]
    fn handle_newline_at_beginning_of_line_puts_empty_line_above() {
        let mut view = make_view(&["abcdef"]);
        view.set_location(0, 0);

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 2);
        assert_eq!(view.buffer.lines[0].grapheme_count(), 0);
        let bottom: String = view.buffer.lines[1].to_string();
        assert_eq!(bottom, "abcdef");
        assert_eq!((view.location.x, view.location.y), (0, 1));
    }

    #[test]
    fn handle_newline_at_end_of_line_puts_empty_line_below() {
        let mut view = make_view(&["abcdef"]);
        view.set_location(6, 0);

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 2);
        let top: String = view.buffer.lines[0].to_string();
        assert_eq!(top, "abcdef");
        assert_eq!(view.buffer.lines[1].grapheme_count(), 0);
        assert_eq!((view.location.x, view.location.y), (0, 1));
    }

    #[test]
    fn handle_newline_at_bottom_of_document_appends_a_single_empty_line() {
        let mut view = make_view(&["abc", "def"]);
        view.set_location(0, 2); // the "one past the end" position

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 3);
        let texts: Vec<String> = view.buffer.lines.iter().map(ToString::to_string).collect();
        assert_eq!(texts, vec!["abc", "def", ""]);
        assert_eq!((view.location.x, view.location.y), (0, 2));
    }

    #[test]
    fn handle_newline_on_empty_buffer_appends_a_single_empty_line() {
        let mut view = make_view(&[]);
        view.set_location(0, 0);

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 1);
        assert_eq!(view.buffer.lines[0].grapheme_count(), 0);
        assert_eq!((view.location.x, view.location.y), (0, 0));
    }

    #[test]
    fn handle_newline_in_middle_of_document_does_not_touch_other_lines() {
        let mut view = make_view(&["first", "second", "third"]);
        view.set_location(2, 1);

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 4);
        let texts: Vec<String> = view.buffer.lines.iter().map(ToString::to_string).collect();
        assert_eq!(texts, vec!["first", "se", "cond", "third"]);
        assert_eq!((view.location.x, view.location.y), (0, 2));
        assert!(view.modified);
    }

    #[test]
    fn handle_newline_keeps_tab_in_upper_line_when_splitting_after_it() {
        let mut view = make_view(&["ab\txyz"]);
        view.set_location(3, 0); // between the tab and 'x'

        view.handle_newline();

        assert_eq!(view.buffer.lines.len(), 2);
        let top: String = view.buffer.lines[0].to_string();
        let bottom: String = view.buffer.lines[1].to_string();
        assert_eq!(top, "ab\t");
        assert_eq!(bottom, "xyz");
    }
}
