use crate::buffer::Buffer;
use crate::terminal::Terminal;

pub struct View {
    buffer: Buffer,
}

impl Default for View {
    fn default() -> Self {
        View {
            buffer: Buffer::default(),
        }
    }
}

impl View {
    pub fn with_buffer(buffer: Buffer) -> Self {
        Self { buffer: buffer }
    }

    pub fn render(&self) -> Result<(), std::io::Error> {
        self.draw_rows()?;
        Self::draw_welcome()?;
        Ok(())
    }

    fn draw_rows(&self) -> Result<(), std::io::Error> {
        let height = Terminal::size()?.height;
        for current_row in 0..height {
            if let Some(line) = self.buffer.lines.get(current_row) {
                Terminal::print_at(0, current_row, line)?;
            } else {
                Terminal::print_at(0, current_row, "~")?;
            }

            if current_row.saturating_add(1) < height {
                Terminal::print("\r\n")?;
            }
        }
        Ok(())
    }
    fn draw_welcome() -> Result<(), std::io::Error> {
        let size = Terminal::size()?;
        let width = size.width;
        let welcome = "Welcome to STED!";
        let sted = "The (S)imple (T)erminal (Ed)itor";
        let version = "Version 1.0.0";

        #[allow(clippy::integer_division)]
        let welcome_row = (size.height / 2).saturating_sub(2);
        // we allow this since we don't care if our welcome message is put _exactly_ in the middle.
        // it's allowed to be a bit to the left or right.
        #[allow(clippy::integer_division)]
        let welcome_col = width.saturating_sub(welcome.len()) / 2;
        let sted_row = welcome_row.saturating_add(1);
        #[allow(clippy::integer_division)]
        let sted_col = width.saturating_sub(sted.len()) / 2;
        let version_row = welcome_row.saturating_add(2);
        #[allow(clippy::integer_division)]
        let version_col = width.saturating_sub(version.len()) / 2;

        Terminal::print_at(welcome_col.saturating_sub(1), welcome_row, welcome)?;

        Terminal::print_at(sted_col.saturating_sub(1), sted_row, sted)?;

        Terminal::print_at(version_col.saturating_sub(1), version_row, version)?;

        Ok(())
    }
}
