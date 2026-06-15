use crate::buffer::Buffer;
use crate::common::types::{Position, Size};
use crate::terminal::Terminal;
use std::cmp::min;

const MAX_LINES_PAST_BUFFER: usize = 10;

pub enum CaretDirection {
    Up,
    Down,
    Left,
    Right,
    Top,
    Bottom,
    LineStart,
    LineEnd,
}

pub struct View {
    buffer: Buffer,
    modified: bool,
    size: Size,
    pub caret_position: Position,
    pub scroll_offset: Position,
}

impl Default for View {
    fn default() -> Self {
        View {
            buffer: Buffer::default(),
            modified: true,
            size: Terminal::size().unwrap_or_default(),
            caret_position: Position::default(),
            scroll_offset: Position::default(),
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
        }
    }

    pub fn render_buffer(&mut self) {
        let height = self.size.height;

        for current_row in 0..height {
            let _ = Terminal::clear_row(current_row);

            if let Some(line) = self
                .buffer
                .lines
                .get(current_row.saturating_add(self.scroll_offset.row))
            {
                let truncated_line = self.get_line_with_x_offset(line);
                let _ = Terminal::print_at(0, current_row, truncated_line);
            } else {
                let _ = Terminal::print_at(0, current_row, "~");
            }

            if current_row.saturating_add(1) < height {
                let _ = Terminal::move_cursor_to(0, current_row.saturating_add(1));
            }
        }

        self.buffer.modified = false;
    }

    fn get_line_with_x_offset<'a>(&self, line: &'a str) -> &'a str {
        let line_start = min(self.scroll_offset.col, line.len());
        let line_end = min(line_start + self.size.width, line.len());
        let offset_line = &line[line_start..line_end];
        offset_line
    }

    pub fn render(&mut self) {
        if self.modified {
            let _ = self.render_buffer();

            if self.buffer.is_empty() {
                let _ = self.render_welcome_screen();
            }

            self.modified = false;
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

    pub fn move_caret_position(&mut self, direction: CaretDirection) {
        // if we're attempting to move out of bounds, only change the offset.
        // Else, modify the caret position.
        match direction {
            CaretDirection::Up => {
                if self.caret_position.row <= 0 {
                    self.scroll_offset.row = self.scroll_offset.row.saturating_sub(1);
                    self.modified = true;
                } else {
                    self.set_caret_position(
                        self.caret_position.col,
                        self.caret_position.row.saturating_sub(1),
                    )
                }
            }
            CaretDirection::Down => {
                // If we scroll past the buffer lines length, only show
                // MAX_LINES_PAST_BUFFER more blank lines.
                if self.scroll_offset.row + self.size.height
                    >= self.buffer.lines.len() + MAX_LINES_PAST_BUFFER
                {
                    () // Do nothing
                }
                // If we're already at the y max of the terminal,
                // leave the cursor at the same y max and increment
                // the y offset.
                else if self.caret_position.row >= self.size.height {
                    self.scroll_offset.row = self.scroll_offset.row.saturating_add(1);
                    self.modified = true;
                } else {
                    self.set_caret_position(
                        self.caret_position.col,
                        self.caret_position.row.saturating_add(1),
                    )
                }
            }
            CaretDirection::Left => {
                if self.caret_position.col <= 0 {
                    self.scroll_offset.col = self.scroll_offset.col.saturating_sub(1);
                    self.modified = true;
                } else {
                    self.set_caret_position(
                        self.caret_position.col.saturating_sub(1),
                        self.caret_position.row,
                    )
                }
            }
            CaretDirection::Right => {
                // If we're already at the x max of the terminal,
                // leave the cursor at the same x max and increment
                // the x offset.
                if self.caret_position.col >= self.size.width {
                    self.scroll_offset.col = self.scroll_offset.col.saturating_add(1);
                    self.modified = true;
                } else {
                    self.set_caret_position(
                        self.caret_position.col.saturating_add(1),
                        self.caret_position.row,
                    )
                }
            }
            CaretDirection::Top => self.set_caret_position(self.caret_position.col, 0),
            CaretDirection::Bottom => {
                self.set_caret_position(self.caret_position.col, self.size.height)
            }
            CaretDirection::LineStart => self.set_caret_position(0, self.caret_position.row),
            CaretDirection::LineEnd => {
                self.set_caret_position(self.size.width, self.caret_position.row)
            }
        }
    }

    pub fn set_caret_position(&mut self, x: usize, y: usize) {
        self.set_caret_position_pos(Position { col: x, row: y })
    }

    pub fn set_caret_position_pos(&mut self, pos: Position) {
        self.caret_position = pos
    }
}
