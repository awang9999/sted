use crate::common::{
    constants::TAB_WIDTH_SPACES,
    textfragment::{GraphemeWidth, TextFragment},
};
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

                let replacement = Line::replacement_character(grapheme);
                TextFragment::from(grapheme, rendered_width, replacement)
            })
            .collect::<Vec<_>>();

        // Merge fragments that form a compound emoji via U+200D.
        let mut merged: Vec<TextFragment> = Vec::with_capacity(fragments.len());
        let mut i = 0;
        while i < fragments.len() {
            let mut current = fragments[i].clone();
            while i + 1 < fragments.len()
                && (fragments[i].grapheme.ends_with('\u{200d}')
                    || fragments[i + 1].grapheme.starts_with('\u{200d}'))
            {
                let next_str = &fragments[i + 1].grapheme;
                let combined_str = format!("{}{}", current.grapheme, next_str);

                let width = UnicodeWidthStr::width(combined_str.as_str());
                let rendered_width = if width > 1 {
                    GraphemeWidth::Full
                } else {
                    GraphemeWidth::Half
                };
                let replacement = Line::replacement_character(combined_str.as_str());

                current.grapheme = combined_str;
                current.rendered_width = rendered_width;
                current.replacement = replacement;

                i += 1; // skip merged fragment
            }
            merged.push(current);
            i += 1;
        }

        Self {
            content: merged,
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

    fn replacement_character(for_str: &str) -> Option<String> {
        let width = for_str.width();
        match for_str {
            // Spaces get rendered as spaces
            " " => None,
            // Tabs get rendered as spaces
            "\t" => Some(" ".repeat(TAB_WIDTH_SPACES)),
            // Any visible whitespace characters besides spaces and tabs
            _ if width > 0 && for_str.trim().is_empty() => Some('␣'.to_string()),
            // Control characters (one or more consecutive)
            _ if for_str.chars().all(|c| c.is_control()) => Some('▯'.to_string()),
            _ => None,
        }
    }
}
