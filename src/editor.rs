use crate::terminal::{Terminal, TerminalCoordinate};
use crossterm::{
    ExecutableCommand,
    event::{
        Event::{self, Key},
        KeyCode::Char,
        KeyEvent, KeyModifiers, read,
    },
};

pub struct Editor {
    should_quit: bool,
}

impl Editor {
    pub const fn default() -> Self {
        Self { should_quit: false }
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
            self.evaluate_event(&event);
        }
        Ok(())
    }
    fn evaluate_event(&mut self, event: &Event) {
        if let Key(KeyEvent {
            code, modifiers, ..
        }) = event
        {
            match code {
                Char('q') if *modifiers == KeyModifiers::CONTROL => {
                    self.should_quit = true;
                }
                _ => (),
            }
        }
    }
    fn refresh_screen(&self) -> Result<(), std::io::Error> {
        Terminal::hide_cursor()?;
        if self.should_quit {
            Terminal::clear_screen()?;
            print!("Goodbye.\r\n");
        } else {
            Terminal::move_cursor_to(TerminalCoordinate { x: 0, y: 0 })?;
            Self::draw_rows()?;
            Terminal::show_cursor()?;
            Terminal::execute()?;
        }
        Ok(())
    }
    fn draw_rows() -> Result<(), std::io::Error> {
        let height = Terminal::size()?.height;
        for current_row in 0..height {
            Terminal::clear_row(current_row)?;
            Terminal::print("~")?;
            if current_row + 1 < height {
                Terminal::print("\r\n")?;
            }
        }
        Ok(())
    }
}
