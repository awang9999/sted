use crate::buffer::Buffer;
use crate::terminal::{Size, Terminal};

pub struct View {
    buffer: Buffer,
    modified: bool,
    size: Size,
}

impl Default for View {
    fn default() -> Self {
        View {
            buffer: Buffer::default(),
            modified: true,
            size: Terminal::size().unwrap_or_default(),
        }
    }
}

impl View {
    pub fn is_modified(&self) -> bool {
        self.modified || self.buffer.modified
    }

    pub fn resize(&mut self, to: Size) {
        self.size = to;
        self.modified = true;
    }

    pub fn get_size(&self) -> Size {
        self.size
    }

    pub fn load(&mut self, file_name: &str) {
        if let Ok(buffer) = Buffer::load(file_name) {
            self.buffer = buffer;
        }
    }

    pub fn render_buffer(&mut self) -> Result<(), std::io::Error> {
        let width = self.size.width;
        let height = self.size.height;

        for current_row in 0..height {
            Terminal::clear_row(current_row)?;

            if let Some(line) = self.buffer.lines.get(current_row) {
                let truncated_line = Self::truncate_line(line, width);
                Terminal::print_at(0, current_row, truncated_line)?;
            } else {
                Terminal::print_at(0, current_row, "~")?;
            }

            if current_row.saturating_add(1) < height {
                Terminal::move_cursor_to(0, current_row.saturating_add(1))?;
            }
        }

        self.buffer.modified = false;

        Ok(())
    }

    fn truncate_line(line: &str, width: usize) -> &str {
        if line.len() < width {
            return line;
        } else {
            let truncated_line = &line[0..width];
            return truncated_line;
        };
    }
    pub fn render(&mut self) -> Result<(), std::io::Error> {
        self.render_buffer()?;

        if self.buffer.is_empty() {
            self.render_welcome_screen()?;
        }

        self.modified = false;

        Ok(())
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
}
