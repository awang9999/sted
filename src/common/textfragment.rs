#[derive(Clone)]
pub enum GraphemeWidth {
    Half,
    Full,
    /// A tab, which renders as `TAB_WIDTH_SPACES` columns of spaces.
    Tab,
}

/// Layout information for one grapheme cluster of a line. The grapheme
/// itself is not stored: it is the slice of the line's text from
/// `byte_index` to the next fragment's `byte_index` (or the end of the
/// text), so the text has a single copy.
#[derive(Clone)]
pub struct TextFragment {
    /// The byte index of this grapheme's first byte within the line's text,
    /// as reported by `grapheme_indices`.
    pub byte_index: usize,
    pub rendered_width: GraphemeWidth,
    pub replacement: Option<String>,
}

impl TextFragment {
    pub fn from(
        byte_index: usize,
        rendered_width: GraphemeWidth,
        replacement: Option<String>,
    ) -> Self {
        Self {
            byte_index,
            rendered_width,
            replacement,
        }
    }
}
