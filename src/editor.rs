use crate::terminal::{Position, Terminal};
use crossterm::event::{
    Event::{self, Key},
    KeyCode::Char,
    KeyEvent, KeyModifiers, read,
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
            Terminal::move_cursor_to(0, 0)?;
            Self::draw_rows()?;
            Self::draw_welcome()?;
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
        let sted_row = welcome_row + 1;
        let sted_col = width.saturating_sub(sted.len()) / 2;
        let version_row = welcome_row + 2;
        let version_col = width.saturating_sub(version.len()) / 2;

        Terminal::print_at(
            0,
            welcome_row,
            &("~".to_string() + &" ".repeat((welcome_col - 1).into()) + welcome),
        )?;

        Terminal::print_at(
            0,
            sted_row,
            &("~".to_string() + &" ".repeat((sted_col - 1).into()) + sted),
        )?;

        Terminal::print_at(
            0,
            version_row,
            &("~".to_string() + &" ".repeat((version_col - 1).into()) + version),
        )?;

        Terminal::move_cursor_to_pos(Position { x: 0, y: 0 })?;

        Ok(())
    }
}
