use crate::common::textfragment::{GraphemeWidth, TextFragment};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone)]
pub struct Line {
    content: Vec<TextFragment>,
    pub scroll_x: usize,
}
impl Line {
    pub fn from(line_str: &str) -> Self {
        let fragments = line_str
            .graphemes(true)
            .map(|grapheme| {
                let unicode_width = grapheme.width();
                let rendered_width = match unicode_width {
                    0 | 1 => GraphemeWidth::Half,
                    _ => GraphemeWidth::Full,
                };

                let replacement = match unicode_width {
                    0 => Some('·'.to_string()),
                    _ => None,
                };

                TextFragment::from(grapheme, rendered_width, replacement)
            })
            .collect();

        Self {
            content: fragments,
            scroll_x: 0,
        }
    }

    // Given a view offset x and a terminal width, returns
    // the visible portion of the Line. If the start or end
    // of the visible range cuts off a grapheme, the character
    // U+2026 (…) replaces it.
    pub fn get(&self, range: Range<usize>) -> String {
        if self.content.is_empty() {
            return String::new();
        }

        let mut segment = String::from("");
        let mut idx = 0; // track index of grapheme vector

        // Skip past graphemes before the visible range
        while idx < self.content.len() && self.width_until(idx) < range.start {
            idx += 1;
        }

        // Truncation at range.start?
        if idx > 0 && self.width_until(idx - 1) < range.start {
            segment.push_str("…");
            idx += 1;
        }

        // Add graphemes within the visible range
        while idx < self.content.len() && self.width_until(idx + 1) <= range.end {
            segment.push_str(&self.content[idx].get_display_text());
            idx += 1;
        }

        // Truncation at range.end?
        if idx < self.content.len() && self.width_until(idx) > range.end {
            segment.push_str("…");
        }

        segment
    }

    pub fn grapheme_count(&self) -> usize {
        return self.content.len();
    }

    pub fn width_until(&self, grapheme_index: usize) -> usize {
        self.content
            .iter()
            .take(grapheme_index)
            .map(|fragment| match fragment.rendered_width {
                GraphemeWidth::Half => 1,
                GraphemeWidth::Full => 2,
            })
            .sum()
    }
}
