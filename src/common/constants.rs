use std::time::Duration;

pub const TAB_WIDTH_SPACES: usize = 4;
/// Rows reserved at the bottom of the terminal for the status bar and its margin.
pub const RESERVED_BOTTOM_LINES: usize = 2;
/// Title of editor in window
pub const EDITOR_TITLE: &str = "STED - Simple Terminal EDitor";
/// How long a routine message (like a save confirmation) stays on the
/// message bar before it disappears on its own.
pub const DEFAULT_MESSAGE_DURATION: Duration = Duration::from_secs(5);
/// How long an important message (like an error or a warning) stays on
/// the message bar: effectively "until the user has read it".
pub const LONG_MESSAGE_DURATION: Duration = Duration::from_secs(100_000);
/// How often the event loop wakes up while idle, so time-based updates
/// (like message bar expiry) fire without user input.
pub const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
