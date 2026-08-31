use unicode_width::UnicodeWidthChar;

pub const TAB_SIZE: usize = 8;

/// Compute the on-screen column of the character at index `upto` in
/// `chars`, honoring tab stops (rounded up to the next multiple of
/// `tab_size`) and double-width East Asian characters.
pub fn display_width(chars: &[char], upto: usize, tab_size: usize) -> usize {
    let mut col = 0usize;
    for &c in chars.iter().take(upto) {
        if c == '\t' {
            col += tab_size - (col % tab_size);
        } else {
            col += UnicodeWidthChar::width(c).unwrap_or(1);
        }
    }
    col
}

/// Width of a single character at display column `at_col` (tabs need to
/// know their current column to know how far they extend).
pub fn char_display_width(c: char, at_col: usize, tab_size: usize) -> usize {
    if c == '\t' {
        tab_size - (at_col % tab_size)
    } else {
        UnicodeWidthChar::width(c).unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_expand_to_next_stop() {
        let chars: Vec<char> = "a\tb".chars().collect();
        // 'a' at col0 -> col1; tab from col1 rounds up to col8; 'b' at col8
        assert_eq!(display_width(&chars, 1, 8), 1);
        assert_eq!(display_width(&chars, 2, 8), 8);
        assert_eq!(display_width(&chars, 3, 8), 9);
    }

    #[test]
    fn cjk_chars_are_double_width() {
        let chars: Vec<char> = "日本語".chars().collect();
        assert_eq!(display_width(&chars, 1, 8), 2);
        assert_eq!(display_width(&chars, 3, 8), 6);
    }

    #[test]
    fn ascii_is_single_width() {
        let chars: Vec<char> = "hello".chars().collect();
        assert_eq!(display_width(&chars, 5, 8), 5);
    }
}
