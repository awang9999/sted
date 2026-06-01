use crossterm::cursor::{Hide, MoveTo, MoveToRow, Show};
use crossterm::style::Print;
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode, size};
use crossterm::{Command, queue};
use std::io::{Error, Write, stdout};

#[derive(Copy, Clone)]
pub struct Size {
    pub height: usize,
    pub width: usize,
}

pub struct Position {
    // Origin is top left corner. Positive x is right, positive y is down
    pub x: usize,
    pub y: usize,
}

/// Represents the Terminal.
/// Edge Case for platforms where `usize` < `u16`:
/// Regardless of the actual size of the Terminal, this representation
/// only spans over at most `usize::MAX` or `u16::size` rows/columns, whichever is smaller.
/// Each size returned truncates to min(`usize::MAX`, `u16::MAX`)
/// And should you attempt to set the cursor out of these bounds, it will also be truncated.
#[derive(Copy, Clone)]
pub struct Terminal;

impl Terminal {
    pub fn terminate() -> Result<(), Error> {
        disable_raw_mode()?;
        Ok(())
    }
    pub fn initialize() -> Result<(), Error> {
        enable_raw_mode()?;
        Self::clear_screen()?;
        Self::move_cursor_to(0, 0)?;
        Self::execute()?;
        Ok(())
    }

    fn queue_command(command: impl Command) -> Result<(), Error> {
        queue!(stdout(), command)?;
        Ok(())
    }
    pub fn clear_screen() -> Result<(), Error> {
        Self::queue_command(Clear(ClearType::All))?;
        Ok(())
    }
    pub fn move_cursor_to(x: usize, y: usize) -> Result<(), Error> {
        let pos = Position { x, y };
        Self::move_cursor_to_pos(&pos)?;
        Ok(())
    }
    /// Moves the cursor to the given Position.
    /// # Arguments
    /// * `Position` - the  `Position`to move the cursor to. Will be truncated to `u16::MAX` if bigger.
    pub fn move_cursor_to_pos(c: &Position) -> Result<(), Error> {
        #[allow(clippy::cast_possible_truncation)]
        Self::queue_command(MoveTo(c.x as u16, c.y as u16))?;
        Ok(())
    }

    /// Returns the current size of this Terminal.
    /// Edge Case for systems with `usize` < `u16`:
    /// * A `Size` representing the terminal size. Any coordinate `z` truncated to `usize` if `usize` < `z` < `u16`
    pub fn size() -> Result<Size, Error> {
        let (w_u16, h_u16) = size()?;
        #[allow(clippy::cast_possible_truncation)]
        Ok(Size {
            width: w_u16 as usize,
            height: h_u16 as usize,
        })
    }
    pub fn hide_cursor() -> Result<(), Error> {
        Self::queue_command(Hide)?;
        Ok(())
    }
    pub fn show_cursor() -> Result<(), Error> {
        Self::queue_command(Show)?;
        Ok(())
    }
    pub fn clear_row(row: usize) -> Result<(), Error> {
        Self::queue_command(MoveToRow(row as u16))?;
        Self::queue_command(Clear(ClearType::CurrentLine))?;
        Ok(())
    }

    pub fn print(string: &str) -> Result<(), Error> {
        Self::queue_command(Print(string))?;
        Ok(())
    }

    pub fn print_at(x: usize, y: usize, string: &str) -> Result<(), Error> {
        let pos = Position { x, y };
        Self::print_at_pos(&pos, string)?;
        Ok(())
    }

    pub fn print_at_pos(pos: &Position, string: &str) -> Result<(), Error> {
        Self::move_cursor_to_pos(pos)?;
        Self::print(string)?;
        Ok(())
    }

    pub fn execute() -> Result<(), Error> {
        stdout().flush()?;
        Ok(())
    }
}
