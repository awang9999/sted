use crossterm::cursor::{Hide, MoveTo, MoveToRow, Show};
use crossterm::{queue, Command};
use crossterm::style::Print;
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode, size};
use std::io::{Write, stdout, Error};

#[derive(Copy, Clone)]
pub struct Size {
    pub height: u16,
    pub width: u16,
}

pub struct Position {
    // Origin is top left corner. Positive x is right, positive y is down
    pub x: u16,
    pub y: u16,
}

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
        Self::move_cursor_to(Position { x: 0, y: 0 })?;
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
    pub fn move_cursor_to(c: Position) -> Result<(), Error> {
        Self::queue_command(MoveTo(c.x, c.y))?;
        Ok(())
    }
    pub fn size() -> Result<Size, Error> {
        let (w, h) = size()?;
        Ok(Size {
            width: w,
            height: h,
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
    pub fn clear_row(row: u16) -> Result<(), Error> {
        Self::queue_command(MoveToRow(row))?;
        Self::queue_command(Clear(ClearType::CurrentLine))?;
        Ok(())
    }

    pub fn print(string: &str) -> Result<(), Error> {
        Self::queue_command(Print(string))?;
        Ok(())
    }

    pub fn print_at(pos: Position, string: &str) -> Result<(), Error> {
        Self::move_cursor_to(pos)?;
        Self::print(string)?;
        Ok(())
    }

    pub fn execute() -> Result<(), Error> {
        stdout().flush()?;
        Ok(())
    }
}
