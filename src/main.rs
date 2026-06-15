#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::print_stdout,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::integer_division
)]

mod buffer;
mod common;
mod editor;
mod terminal;
mod view;

use editor::Editor;

fn main() {
    Editor::new().unwrap().run();
}
