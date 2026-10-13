use crate::common::constants::RESERVED_BOTTOM_LINES;
use crate::terminal::Size;

/// Row offset of the status bar: the first of the rows reserved at the
/// bottom of the terminal.
pub const STATUS_BAR_OFFSET: usize = 0;
/// Row offset of the message bar — and of any prompt that temporarily
/// replaces it: the last row of the terminal.
pub const MESSAGE_BAR_OFFSET: usize = 1;

/// The width and terminal row of a bar anchored to the rows reserved at
/// the bottom of the terminal.
///
/// A bar's `offset` is its distance above the first reserved row, so the
/// invariant "bars live on the reserved rows, and never above them" is
/// defined once here instead of being re-derived in every bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarLayout {
    width: usize,
    row: usize,
    offset: usize,
}

impl BarLayout {
    pub fn new(size: Size, offset: usize) -> Self {
        Self {
            width: size.width,
            row: Self::row_for(size, offset),
            offset,
        }
    }

    pub fn resize(&mut self, size: Size) {
        self.width = size.width;
        self.row = Self::row_for(size, self.offset);
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn row(&self) -> usize {
        self.row
    }

    fn row_for(size: Size, offset: usize) -> usize {
        size.height
            .saturating_sub(RESERVED_BOTTOM_LINES)
            .saturating_add(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_bar_sits_on_the_first_reserved_row() {
        let layout = BarLayout::new(
            Size {
                width: 80,
                height: 24,
            },
            STATUS_BAR_OFFSET,
        );
        assert_eq!(layout.row(), 22);
        assert_eq!(layout.width(), 80);
    }

    #[test]
    fn message_bar_sits_on_the_last_row() {
        let layout = BarLayout::new(
            Size {
                width: 80,
                height: 24,
            },
            MESSAGE_BAR_OFFSET,
        );
        assert_eq!(layout.row(), 23);
    }

    #[test]
    fn resize_moves_the_bar_and_updates_the_width() {
        let mut layout = BarLayout::new(
            Size {
                width: 80,
                height: 24,
            },
            MESSAGE_BAR_OFFSET,
        );
        layout.resize(Size {
            width: 100,
            height: 30,
        });
        assert_eq!(layout.row(), 29);
        assert_eq!(layout.width(), 100);
    }

    #[test]
    fn row_saturates_when_the_terminal_is_shorter_than_the_reserved_rows() {
        let layout = BarLayout::new(
            Size {
                width: 80,
                height: 0,
            },
            MESSAGE_BAR_OFFSET,
        );
        assert_eq!(layout.row(), 1);
    }
}
