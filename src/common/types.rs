use crate::terminal::Position;

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct Location {
    pub x: usize,
    pub y: usize,
}

impl From<Location> for Position {
    fn from(loc: Location) -> Self {
        Self {
            col: loc.x,
            row: loc.y,
        }
    }
}
