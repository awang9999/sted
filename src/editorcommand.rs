use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use std::convert::TryFrom;

use super::terminal::Size;

pub enum Direction {
    PageUp,
    PageDown,
    Home,
    End,
    Up,
    Left,
    Right,
    Down,
}

pub enum EditorCommand {
    Move(Direction),
    Resize(Size),
    Insert(char),
    NewLine,
    Delete,
    BackSpace,
    Quit,
}

impl TryFrom<Event> for EditorCommand {
    type Error = String;

    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match event {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match (code, modifiers) {
                // Program control
                (KeyCode::Char('q'), KeyModifiers::CONTROL) => Ok(Self::Quit),
                // Caret movement
                (KeyCode::Up, KeyModifiers::NONE) => Ok(Self::Move(Direction::Up)),
                (KeyCode::Down, KeyModifiers::NONE) => Ok(Self::Move(Direction::Down)),
                (KeyCode::Left, KeyModifiers::NONE) => Ok(Self::Move(Direction::Left)),
                (KeyCode::Right, KeyModifiers::NONE) => Ok(Self::Move(Direction::Right)),
                (KeyCode::PageDown, KeyModifiers::NONE) => Ok(Self::Move(Direction::PageDown)),
                (KeyCode::PageUp, KeyModifiers::NONE) => Ok(Self::Move(Direction::PageUp)),
                (KeyCode::Home, KeyModifiers::NONE) => Ok(Self::Move(Direction::Home)),
                (KeyCode::End, KeyModifiers::NONE) => Ok(Self::Move(Direction::End)),
                // Ordinary presses
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => Ok(Self::Insert(c)),
                // Enter
                (KeyCode::Enter, KeyModifiers::NONE) => Ok(Self::NewLine),
                // Delete
                (KeyCode::Delete, KeyModifiers::NONE) => Ok(Self::Delete),
                // Backspace
                (KeyCode::Backspace, KeyModifiers::NONE) => Ok(Self::BackSpace),
                _ => Err(format!("Key Code not supported: {code:?}")),
            },
            Event::Resize(width_u16, height_u16) => {
                // clippy::as_conversions: Will run into problems for rare edge case systems where usize < u16
                #[allow(clippy::as_conversions)]
                let height = height_u16 as usize;
                // clippy::as_conversions: Will run into problems for rare edge case systems where usize < u16
                #[allow(clippy::as_conversions)]
                let width = width_u16 as usize;
                Ok(Self::Resize(Size { height, width }))
            }
            _ => Err(format!("Event not supported: {event:?}")),
        }
    }
}
