use std::cmp::min;
use std::ops::Range;

#[derive(Default, Copy, Clone)]
pub struct Size {
    pub height: usize,
    pub width: usize,
}

#[derive(Default, Copy, Clone)]
pub struct Position {
    // Origin is top left corner. Positive x is right, positive y is down
    pub row: usize,
    pub col: usize,
}

pub struct Location {
    pub x: usize,
    pub y: usize,
}

impl From<Location> for Position {
    fn from(loc: Location) -> Self {
        Self {
            row: loc.x,
            col: loc.y,
        }
    }
}

impl Location {
    pub const fn subtract(&self, other: &Self) -> Self {
        Self {
            x: self.x.saturating_sub(other.x),
            y: self.y.saturating_sub(other.y),
        }
    }
}

pub struct Line {
    string: String,
}
impl Line {
    pub fn from(line_str: &str) -> Self {
        Self {
            string: String::from(line_str),
        }
    }

    pub fn get(&self, range: Range<usize>) -> String {
        let start = range.start;
        let end = min(range.end, self.string.len());
        self.string.get(start..end).unwrap_or_default().to_string()
    }
}
