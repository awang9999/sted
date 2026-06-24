use std::cmp::min;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
pub struct Line {
    string: String,
    pub scroll_x: usize,
}
impl Line {
    pub fn from(line_str: &str) -> Self {
        Self {
            string: String::from(line_str),
            scroll_x: 0,
        }
    }

    pub fn get(&self, range: Range<usize>) -> String {
        let graphemes = self.string[..].graphemes(true).collect::<Vec<&str>>();
        let graphemes_len = graphemes.len();
        let start = min(range.start, graphemes_len);
        let end = min(range.end, graphemes_len);

        // If the range is completely out of bounds or inverted, return an empty string
        if start >= end {
            return String::new();
        }

        graphemes[start..end].concat()
    }

    pub fn len(&self) -> usize {
        self.string[..].graphemes(true).count()
    }
}
