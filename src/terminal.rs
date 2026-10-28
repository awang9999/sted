use crossterm::cursor::{Hide, MoveTo, MoveToRow, Show};
use crossterm::style::{Attribute, Color, Print, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{
    Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen,
    SetTitle, disable_raw_mode, enable_raw_mode, size,
};
use crossterm::{Command, queue};
use std::io::{Error, Write, stdout};

use crate::common::constants::EDITOR_TITLE;

// A Size here represents the width and height of the Terminal in characters
#[derive(Default, Copy, Clone)]
pub struct Size {
    pub height: usize,
    pub width: usize,
}
// A Position here represents a location relative to the top left of the terminal window.
// col: 0, row: 0 represents the top left character.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub struct Position {
    pub col: usize,
    pub row: usize,
}

impl Position {
    pub const fn subtract(&self, other: &Self) -> Self {
        Self {
            col: self.col.saturating_sub(other.col),
            row: self.row.saturating_sub(other.row),
        }
    }
}

/// Represents the Terminal.
/// Coordinates are tracked as `usize`. When a coordinate must be handed to
/// the terminal (which only accepts `u16`), values beyond `u16::MAX` are
/// clamped to `u16::MAX` instead of being silently truncated.
#[derive(Copy, Clone)]
pub struct Terminal;

impl Terminal {
    pub fn terminate() -> Result<(), Error> {
        Self::queue_command(EnableLineWrap)?;
        Self::queue_command(LeaveAlternateScreen)?;
        Self::show_cursor()?;
        Self::execute()?;
        disable_raw_mode()?;
        Ok(())
    }
    pub fn initialize() -> Result<(), Error> {
        enable_raw_mode()?;
        Self::queue_command(EnterAlternateScreen)?;
        Self::clear_screen()?;
        Self::queue_command(DisableLineWrap)?;
        Self::queue_command(SetTitle(EDITOR_TITLE))?;
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
        let pos = Position { col: x, row: y };
        Self::move_cursor_to_pos(&pos)?;
        Ok(())
    }
    /// Moves the cursor to the given Position.
    /// # Arguments
    /// * `Position` - the  `Position`to move the cursor to. Will be truncated to `u16::MAX` if bigger.
    pub fn move_cursor_to_pos(c: &Position) -> Result<(), Error> {
        Self::queue_command(MoveTo(Self::clamp_to_u16(c.col), Self::clamp_to_u16(c.row)))?;
        Ok(())
    }

    /// Returns the current size of this Terminal.
    /// * A `Size` representing the terminal size. `u16` always fits in
    ///   `usize`, so the conversion is exact on every Rust target.
    pub fn size() -> Result<Size, Error> {
        let (w_u16, h_u16) = size()?;
        Ok(Size {
            width: usize::from(w_u16),
            height: usize::from(h_u16),
        })
    }

    /// Clamps a coordinate to the `u16` range, mapping any value beyond
    /// `u16::MAX` to `u16::MAX` instead of silently truncating it.
    fn clamp_to_u16(value: usize) -> u16 {
        u16::try_from(value).unwrap_or(u16::MAX)
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
        Self::queue_command(MoveToRow(Self::clamp_to_u16(row)))?;
        Self::queue_command(Clear(ClearType::CurrentLine))?;
        Ok(())
    }

    pub fn print(string: &str) -> Result<(), Error> {
        Self::queue_command(Print(string))?;
        Ok(())
    }

    pub fn print_at(x: usize, y: usize, string: &str) -> Result<(), Error> {
        let pos = Position { col: x, row: y };
        Self::print_at_pos(&pos, string)?;
        Ok(())
    }

    pub fn print_row(row: usize, string: &str) -> Result<(), Error> {
        Self::print_at(0, row, string)?;
        Ok(())
    }

    pub fn print_at_pos(pos: &Position, string: &str) -> Result<(), Error> {
        Self::move_cursor_to_pos(pos)?;
        Self::print(string)?;
        Ok(())
    }

    pub fn print_inverted_row(row: usize, string: &str) -> Result<(), Error> {
        let width = Self::size()?.width;
        let inverted_str = format!(
            "{}{:width$.width$}{}",
            Attribute::Reverse,
            string,
            Attribute::Reset
        );
        Self::print_at(0, row, &inverted_str)?;
        Ok(())
    }

    /// Prints `string` at the cursor's current position with the given
    /// foreground and background colors, continuing the row rather than
    /// rewriting it. The reset at the end restores the terminal's default
    /// colors, so the row's following segments are drawn plain.
    fn print_styled(string: &str, foreground: Color, background: Color) -> Result<(), Error> {
        let styled_str = format!(
            "{}{}{}{}",
            SetForegroundColor(foreground),
            SetBackgroundColor(background),
            string,
            Attribute::Reset
        );
        Self::print(&styled_str)?;
        Ok(())
    }

    /// Prints a search match at the cursor's current position: black text
    /// on a warm mid-gray background, continuing the row rather than
    /// rewriting it. The warm gray carries a subtle yellow tinge, bright
    /// enough to stand out on a dark terminal while staying clearly below
    /// the active match's lighter warm gray.
    pub fn print_match(string: &str) -> Result<(), Error> {
        Self::print_styled(
            string,
            Color::Rgb { r: 0, g: 0, b: 0 },
            Color::Rgb {
                r: 153,
                g: 153,
                b: 102,
            },
        )
    }

    /// Prints the active search match — the one the caret sits on — at the
    /// cursor's current position: bold dark gray text on a warm light-gray
    /// background with a subtle yellow tinge, popping brighter than the
    /// other matches.
    pub fn print_active_match(string: &str) -> Result<(), Error> {
        let styled_str = format!(
            "{}{}{}{}{}",
            Attribute::Bold,
            SetForegroundColor(Color::Rgb {
                r: 64,
                g: 64,
                b: 64,
            }),
            SetBackgroundColor(Color::Rgb {
                r: 220,
                g: 220,
                b: 169,
            }),
            string,
            Attribute::Reset
        );
        Self::print(&styled_str)?;
        Ok(())
    }

    pub fn execute() -> Result<(), Error> {
        stdout().flush()?;
        Ok(())
    }
}
