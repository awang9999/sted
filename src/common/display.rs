use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Truncates `text` to at most `width` display columns, never cutting a
/// character in half.
///
/// Width is measured in terminal columns, so full-width graphemes count
/// as two. Truncating by byte count (as in [`str::truncate`]) would both
/// overflow a bar for wide characters and panic when the cut lands in the
/// middle of a multi-byte character.
pub fn fit_to_width(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }

    let mut truncated = String::new();
    let mut columns: usize = 0;
    for ch in text.chars() {
        let char_width = ch.width().unwrap_or(0);
        if columns.saturating_add(char_width) > width {
            break;
        }
        columns = columns.saturating_add(char_width);
        truncated.push(ch);
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_untouched() {
        assert_eq!(fit_to_width("hello", 80), "hello");
    }

    #[test]
    fn long_text_is_cut_to_the_width() {
        assert_eq!(fit_to_width("01234567890123456789", 10), "0123456789");
    }

    #[test]
    fn full_width_graphemes_count_as_two_columns() {
        // Each CJK character occupies two columns, so only two fit in
        // five.
        assert_eq!(fit_to_width("中文中", 5), "中文");
    }

    #[test]
    fn a_multi_byte_character_is_never_cut_in_half() {
        // 'é' is two bytes in UTF-8; a byte-wise truncate at index 4 would
        // panic mid-character.
        assert_eq!(fit_to_width("café", 3), "caf");
        assert_eq!(fit_to_width("café", 4), "café");
    }

    #[test]
    fn zero_width_yields_empty_text() {
        assert_eq!(fit_to_width("hello", 0), "");
    }
}
