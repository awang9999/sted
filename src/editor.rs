use crate::buffer::Buffer;
use crate::terminal::{Position, Terminal};
use crate::view::View;
use core::cmp::{max, min};
use crossterm::event::{
    Event::{self, Key},
    KeyCode,
    KeyCode::Char,
    KeyEvent, KeyModifiers, read,
};

pub struct Editor {
    should_quit: bool,
    position: Position,
    view: View,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            should_quit: false,
            position: Position { x: 0, y: 0 },
            view: View::default(),
        }
    }
}

impl Editor {
    pub fn with_file(file_path: &str) -> Result<Self, std::io::Error> {
        let mut buf = Buffer::default();

        match std::fs::read_to_string(file_path) {
            Ok(file_content) => {
                for line in file_content.lines() {
                    buf.lines.push(String::from(line));
                }
            }
            Err(error) => {
                buf.lines.push(String::from(format!(
                    "Failed to read file at {}",
                    file_path
                )));
                buf.lines
                    .push(String::from(format!("Error message: {}", error)));
            }
        }

        let view = View::with_buffer(buf);

        return Ok(Self {
            should_quit: false,
            position: Position { x: 0, y: 0 },
            view: view,
        });
    }

    pub fn run(&mut self) {
        Terminal::initialize().unwrap();
        let result = self.repl();
        Terminal::terminate().unwrap();
        result.unwrap();
    }

    fn repl(&mut self) -> Result<(), std::io::Error> {
        loop {
            self.refresh_screen()?;
            if self.should_quit {
                break;
            }
            let event = read()?;
            self.evaluate_event(&event)?;
        }
        Ok(())
    }
    fn evaluate_event(&mut self, event: &Event) -> Result<(), std::io::Error> {
        if let Key(KeyEvent {
            code, modifiers, ..
        }) = event
        {
            let term_size = Terminal::size()?;

            match code {
                KeyCode::Up => self.position.y = max(0, self.position.y.saturating_sub(1)),
                KeyCode::Down => {
                    self.position.y = min(term_size.height, self.position.y.saturating_add(1));
                }
                KeyCode::Left => self.position.x = max(0, self.position.x.saturating_sub(1)),
                KeyCode::Right => {
                    self.position.x = min(term_size.width, self.position.x.saturating_add(1));
                }
                KeyCode::PageUp => self.position.y = 0,
                KeyCode::PageDown => self.position.y = term_size.height,
                KeyCode::Home => self.position.x = 0,
                KeyCode::End => self.position.x = term_size.width,
                Char('q') if *modifiers == KeyModifiers::CONTROL => {
                    self.should_quit = true;
                }
                _ => (),
            }
        }
        Ok(())
    }
    fn refresh_screen(&self) -> Result<(), std::io::Error> {
        Terminal::hide_cursor()?;
        if self.should_quit {
            Terminal::clear_screen()?;
            Terminal::move_cursor_to(0, 0)?;
            Terminal::print("Goodbye. \r\n")?;
        } else {
            self.view.render()?;
            Terminal::move_cursor_to_pos(&self.position)?;
        }
        Terminal::show_cursor()?;
        Terminal::execute()?;
        Ok(())
    }
}
