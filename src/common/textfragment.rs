use crate::common::constants::TAB_WIDTH_SPACES;

#[derive(Clone, Copy)]
pub enum GraphemeWidth {
    Half,
    Full,
    /// A tab, which renders as `TAB_WIDTH_SPACES` columns of spaces.
    Tab,
}

impl GraphemeWidth {
    /// The number of terminal columns the grapheme occupies.
    pub const fn columns(self) -> usize {
        match self {
            GraphemeWidth::Half => 1,
            GraphemeWidth::Full => 2,
            GraphemeWidth::Tab => TAB_WIDTH_SPACES,
        }
    }
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
