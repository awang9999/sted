use crate::editorcommand::EditorCommand;
use crate::statusbar::StatusBar;
use crate::terminal::Terminal;
use crate::view::View;
use crossterm::event::{
    Event::{self, Key, Resize},
    read,
};

pub struct Editor {
    should_quit: bool,
    view: View,
    status_bar: StatusBar,
}

impl Default for Editor {
    fn default() -> Self {
        let terminal_size = Terminal::size().unwrap_or_default();
        Self {
            should_quit: false,
            view: View::new(terminal_size),
            status_bar: StatusBar::new(terminal_size),
        }
    }
}

impl Editor {
    pub fn new() -> Result<Self, std::io::Error> {
        let current_hook = std::panic::take_hook();

        std::panic::set_hook(Box::new(move |panic_info| {
            //do the cleanup work
            let _ = Terminal::terminate();
            current_hook(panic_info);
        }));

        Terminal::initialize()?;

        let terminal_size = Terminal::size().unwrap_or_default();
        let mut view = View::new(terminal_size);
        let status_bar = StatusBar::new(terminal_size);

        let args: Vec<String> = std::env::args().collect();

        if let Some(file_name) = args.get(1) {
            let _ = view.load(file_name);
        }

        Ok(Self {
            should_quit: false,
            view,
            status_bar,
        })
    }

    pub fn run(&mut self) {
        // Sync before the first frame, otherwise the bar would render its
        // default (empty) state and stay stale until the first event.
        self.status_bar.update(self.view.get_status());

        loop {
            self.refresh_screen();
            if self.should_quit {
                break;
            }

            match read() {
                Ok(event) => self.evaluate_event(event),
                Err(err) => {
                    #[cfg(debug_assertions)]
                    {
                        panic!("Could not read event: {err:?}");
                    }
                }
            }

            self.status_bar.update(self.view.get_status());
        }
    }

    fn evaluate_event(&mut self, event: Event) {
        let should_process = match event {
            Key(_) => true,
            Resize(_, _) => true,
            _ => false,
        };

        if should_process {
            match EditorCommand::try_from(event) {
                Ok(command) => {
                    if matches!(command, EditorCommand::Quit) {
                        self.should_quit = true;
                    } else {
                        self.view.handle_command(command);
                        if let EditorCommand::Resize(size) = command {
                            self.status_bar.resize(size);
                        }
                    }
                }
                Err(err) => {
                    #[cfg(debug_assertions)]
                    {
                        print!("Could not handle command: {err}");
                    }
                }
            }
        } else {
            #[cfg(debug_assertions)]
            {
                panic!("Received and discarded unsupported or non-press event.");
            }
        }
    }

    fn refresh_screen(&mut self) {
        let _ = Terminal::hide_cursor();
        self.view.render();
        self.status_bar.render();
        let _ = Terminal::move_cursor_to_pos(&self.view.get_caret_position());
        let _ = Terminal::show_cursor();
        let _ = Terminal::execute();
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        let _ = Terminal::terminate();
        if self.should_quit {
            let _ = Terminal::print("Goodbye. \r\n");
        }
    }
}
