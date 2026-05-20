use crossterm::cursor::{Hide, MoveTo, MoveToRow, Show};
use crossterm::queue;
use crossterm::style::Print;
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode, size};
use std::io::{Write, stdout};

#[derive(Copy, Clone)]
pub struct TerminalSize {
    pub height: u16,
    pub width: u16,
}

pub struct TerminalCoordinate {
    pub x: u16,
    pub y: u16,
}

#[derive(Copy, Clone)]
pub struct Terminal {}

impl Terminal {
    pub fn terminate() -> Result<(), std::io::Error> {
        disable_raw_mode()?;
        Ok(())
    }
    pub fn initialize() -> Result<(), std::io::Error> {
        enable_raw_mode()?;
        Self::clear_screen()?;
        Self::move_cursor_to(TerminalCoordinate { x: 0, y: 0 })?;
        Self::execute()?;
        Ok(())
    }
    pub fn clear_screen() -> Result<(), std::io::Error> {
        queue!(stdout(), Clear(ClearType::All))?;
        Ok(())
    }
    pub fn move_cursor_to(c: TerminalCoordinate) -> Result<(), std::io::Error> {
        queue!(stdout(), MoveTo(c.x, c.y))?;
        Ok(())
    }
    pub fn size() -> Result<TerminalSize, std::io::Error> {
        let (w, h) = size()?;
        Ok(TerminalSize {
            width: w,
            height: h,
        })
    }
    pub fn hide_cursor() -> Result<(), std::io::Error> {
        queue!(stdout(), Hide)?;
        Ok(())
    }
    pub fn show_cursor() -> Result<(), std::io::Error> {
        queue!(stdout(), Show)?;
        Ok(())
    }
    pub fn clear_row(row: u16) -> Result<(), std::io::Error> {
        queue!(stdout(), MoveToRow(row), Clear(ClearType::CurrentLine))?;
        Ok(())
    }

    pub fn print(string: &str) -> Result<(), std::io::Error> {
        queue!(stdout(), Print(string))?;
        Ok(())
    }

    pub fn execute() -> Result<(), std::io::Error> {
        stdout().flush()?;
        Ok(())
    }
}
