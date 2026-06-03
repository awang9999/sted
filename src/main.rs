#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::print_stdout,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::integer_division
)]

mod buffer;
mod editor;
mod terminal;
mod view;

use editor::Editor;

fn main() -> Result<(), std::io::Error> {
    let args: Vec<String> = std::env::args().collect();

    if let Some(filename) = args.get(1) {
        Editor::with_file(filename)?.run();
    } else {
        Editor::default().run();
    }

    Ok(())
}
