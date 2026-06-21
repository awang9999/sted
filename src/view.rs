use crate::buffer::Buffer;
use crate::common::types::Location;
use crate::editorcommand::{Direction, EditorCommand};
use crate::terminal::{Position, Size, Terminal};

pub struct View {
    buffer: Buffer,
    modified: bool,
    size: Size,
    pub location: Location,
    pub scroll_offset: Location,
}

impl Default for View {
    fn default() -> Self {
        View {
            buffer: Buffer::default(),
            modified: true,
            size: Terminal::size().unwrap_or_default(),
            location: Location::default(),
            scroll_offset: Location::default(),
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
        match direction {
            Direction::Up => y = y.saturating_sub(1),
            Direction::Down => y = y.saturating_add(1),
            Direction::Left => x = x.saturating_sub(1),
            Direction::Right => x = x.saturating_add(1),
            Direction::BufferTop => y = 0,
            Direction::BufferBottom => {
                // sets location y to the last line of the buffer
                y = self.buffer.lines.len().saturating_sub(1);
            }
            Direction::LineStart => x = 0,
            Direction::LineEnd => {
                // sets location x to the last character of the line
                if let Some(line) = self.buffer.lines.get(self.location.y) {
                    x = line.len();
                }
            }
        }
        self.set_location(x, y);
        self.scroll_location_into_view();
    }

    pub fn scroll_location_into_view(&mut self) {
        let Location { x, y } = self.location;
        let Location {
            x: mut scroll_x,
            y: mut scroll_y,
        } = self.scroll_offset;
        let Size { width, height } = self.size;
        let mut offset_changed = false;

        // Vertical scroll
        // If location y is less than scroll y, set scroll y to location y.
        if y < scroll_y {
            scroll_y = y;
            offset_changed = true;
        }
        // If location y is more than scroll y + view height
        // set scroll y to be one more than location y minus height
        // This finds the "top left y of the viewport" relative to
        // the buffer origin.
        else if y >= scroll_y.saturating_add(height) {
            scroll_y = y.saturating_sub(height).saturating_add(1);
            offset_changed = true;
        }

        // Horizontal scroll
        if x < scroll_x {
            scroll_x = x;
            offset_changed = true;
        } else if x >= scroll_x.saturating_add(width) {
            scroll_x = x.saturating_sub(width).saturating_add(1);
            offset_changed = true;
        }

        self.set_scroll_offset(scroll_x, scroll_y);
        self.modified = offset_changed;
    }

    pub fn set_location(&mut self, x: usize, y: usize) {
        self.set_location_coord(Location { x: x, y: y })
    }

    pub fn set_location_coord(&mut self, pos: Location) {
        self.location = pos
    }

    pub fn set_scroll_offset(&mut self, x: usize, y: usize) {
        self.set_scroll_offset_coord(Location { x: x, y: y })
    }

    pub fn set_scroll_offset_coord(&mut self, pos: Location) {
        self.scroll_offset = pos
    }
}
