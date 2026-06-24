use crate::buffer::Buffer;
use crate::common::types::Location;
use crate::editorcommand::{Direction, EditorCommand};
use crate::terminal::{Position, Size, Terminal};
use std::cmp::min;

pub struct View {
    buffer: Buffer,
    modified: bool,
    size: Size,
    pub location: Location,
    pub scroll_offset: Location,
    desired_x: usize,
}

impl Default for View {
    fn default() -> Self {
        View {
            buffer: Buffer::default(),
            modified: true,
            size: Terminal::size().unwrap_or_default(),
            location: Location::default(),
            scroll_offset: Location::default(),
            desired_x: 0,
        }
    }
}

impl View {
    pub fn resize(&mut self, to: Size) {
        self.size = to;
        self.modified = true;
    }

    pub fn load(&mut self, file_name: &str) {
        if let Ok(buffer) = Buffer::load(file_name) {
            self.buffer = buffer;
            self.modified = true;
        }
    }

    pub fn get_position(&self) -> Position {
        self.location.subtract(&self.scroll_offset).into()
    }

    pub fn handle_command(&mut self, event: EditorCommand) {
        match event {
            EditorCommand::Resize(size) => self.resize(size),
            EditorCommand::Move(direction) => self.move_location(direction),
            EditorCommand::Quit => (),
        };
    }

    pub fn render_buffer(&mut self) {
        let Size { height, width } = self.size;
        if height == 0 || width == 0 {
            return;
        }

        for current_row in 0..height {
            let _ = Terminal::clear_row(current_row);
            let top = self.scroll_offset.y;

            if let Some(line) = self.buffer.lines.get(current_row.saturating_add(top)) {
                let left = self.scroll_offset.x;
                let right = self.scroll_offset.x.saturating_add(width);
                let truncated_line = &line.get(left..right);
                let _ = Self::render_line(current_row, truncated_line);
            } else {
                let _ = Self::render_line(current_row, "~");
            }

            if current_row.saturating_add(1) < height {
                let _ = Terminal::move_cursor_to(0, current_row.saturating_add(1));
            }
        }

        self.buffer.modified = false;
    }

    fn render_line(row: usize, line_text: &str) {
        let result = Terminal::print_row(row, line_text);
        debug_assert!(result.is_ok(), "Failed to render line");
    }

    pub fn render(&mut self) {
        if !self.modified {
            return;
        }

        let _ = self.render_buffer();

        if self.buffer.is_empty() {
            let _ = self.render_welcome_screen();
        }

        self.modified = false;
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
            Direction::Down => y = y.saturating_add(1).min(buffer_length),
            Direction::PageUp => y = y.saturating_sub(height),
            Direction::PageDown => y = (y.saturating_add(height)).min(buffer_length),

            // Horizontal movements
            Direction::Left => {
                if x <= 0 && y > 0 {
                    y = y.saturating_sub(1);
                    x = self.buffer.lines.get(y).map(|l| l.len()).unwrap_or(0);
                } else {
                    x = x.saturating_sub(1);
                }
                self.desired_x = x;
            }

            Direction::Right => {
                if let Some(len) = self.buffer.lines.get(y).map(|l| l.len()) {
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
                if let Some(len) = self.buffer.lines.get(y).map(|l| l.len()) {
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
                .map(|l| min(self.desired_x, l.len()))
                .unwrap_or(0);
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
        if y < scroll.y {
            scroll.y = y;
        }
        // If cursor y is below the viewport, push the viewport down by the offset
        else if y >= scroll.y.saturating_add(height) {
            scroll.y = y.saturating_sub(height).saturating_add(1);
        }

        // Horizontal scroll
        // Use the target line's saved horizontal offset as the baseline
        let line_scroll_x = self.buffer.lines.get(y).map(|l| l.scroll_x).unwrap_or(0);
        let line_len = self.buffer.lines.get(y).map(|l| l.len()).unwrap_or(0);

        // If cursor x is left of the baseline, scroll left to the cursor
        if x < line_scroll_x {
            scroll.x = x;
        }
        // If cursor x is right of the viewport, scroll right to keep it visible
        else if x >= line_scroll_x.saturating_add(width) {
            scroll.x = x.saturating_sub(width).saturating_add(1);
        // Use the saved line scroll to keep the cursor relative to the last time the
        // user was on this line.
        } else {
            scroll.x = line_scroll_x;
        }

        // Clamp scroll_x to the line's length so we never scroll past the visible content
        let max_scroll_x = line_len.saturating_sub(width);
        scroll.x = scroll.x.min(max_scroll_x);

        // Persist the updated horizontal scroll state to the target line
        if let Some(line) = self.buffer.lines.get_mut(y) {
            line.scroll_x = scroll.x;
        }

        // Apply the new scroll offset and mark dirty if the view changed
        self.scroll_offset = scroll;
        self.modified = scroll.x != prev.x || scroll.y != prev.y;
    }

    pub fn set_location(&mut self, x: usize, y: usize) {
        self.set_location_coord(Location { x: x, y: y })
    }

    pub fn set_location_coord(&mut self, pos: Location) {
        self.location = pos
    }
}
