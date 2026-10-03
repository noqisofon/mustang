/// Markdown inline decoration definitions and parser.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationKind {
    Bold,
    Italic,
}

pub struct MarkupSyntax {
    pub bold_marker: &'static str,
    pub italic_marker: &'static str,
}

pub const PANDOC_MARKDOWN: MarkupSyntax = MarkupSyntax {
    bold_marker: "**",
    italic_marker: "*",
};

impl MarkupSyntax {
    pub fn marker_for(&self, kind: DecorationKind) -> &'static str {
        match kind {
            DecorationKind::Bold => self.bold_marker,
            DecorationKind::Italic => self.italic_marker,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharStyle {
    pub is_marker: bool,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchedSpan {
    pub kind: DecorationKind,
    pub open_start: usize,
    pub open_end: usize,
    pub close_start: usize,
    pub close_end: usize,
}

/// Parse the character styles for a single line of text according to `syntax`.
pub fn parse_line_styles(line: &[char], syntax: &MarkupSyntax) -> Vec<CharStyle> {
    let mut styles = vec![CharStyle::default(); line.len()];
    if line.is_empty() {
        return styles;
    }

    let matched_spans = find_matched_spans(line, syntax);
    for span in matched_spans {
        for i in span.open_start..span.open_end {
            styles[i].is_marker = true;
        }
        for i in span.close_start..span.close_end {
            styles[i].is_marker = true;
        }
        for i in span.open_end..span.close_start {
            match span.kind {
                DecorationKind::Bold => styles[i].bold = true,
                DecorationKind::Italic => styles[i].italic = true,
            }
        }
    }

    styles
}

/// Find an enclosing decorated span for `kind` at the given character column `col`.
/// If cursor `col` falls within `open_start..=close_end`, returns the innermost span.
pub fn find_enclosing_decoration_span(
    line: &[char],
    col: usize,
    kind: DecorationKind,
    syntax: &MarkupSyntax,
) -> Option<MatchedSpan> {
    let marker: Vec<char> = syntax.marker_for(kind).chars().collect();
    let m_len = marker.len();
    let pair_len = m_len * 2;

    // First check if col is inside an empty marker pair (e.g. `****` for bold or `**` for italic)
    if line.len() >= pair_len {
        let min_start = col.saturating_sub(pair_len);
        let max_start = col.min(line.len() - pair_len);
        for start in min_start..=max_start {
            if line[start..start + m_len] == marker[..]
                && line[start + m_len..start + pair_len] == marker[..]
            {
                // Ensure for italic that we don't misidentify part of a bold **** as empty italic **
                if kind == DecorationKind::Italic {
                    let before_is_star = start > 0 && line[start - 1] == '*';
                    let after_is_star = start + pair_len < line.len() && line[start + pair_len] == '*';
                    if before_is_star || after_is_star {
                        continue;
                    }
                }
                return Some(MatchedSpan {
                    kind,
                    open_start: start,
                    open_end: start + m_len,
                    close_start: start + m_len,
                    close_end: start + pair_len,
                });
            }
        }
    }

    let spans = find_matched_spans(line, syntax);
    spans
        .into_iter()
        .filter(|s| s.kind == kind && col >= s.open_start && col <= s.close_end)
        .min_by_key(|s| s.close_end - s.open_start)
}

/// Find all code spans on `line` as `(start, end)` character ranges.
fn find_code_spans(line: &[char]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut i = 0;
    let n = line.len();

    while i < n {
        if line[i] == '`' {
            let start = i;
            let mut k = 0;
            while i < n && line[i] == '`' {
                k += 1;
                i += 1;
            }
            // Search for closing run of backticks of exact length k
            let mut j = i;
            let mut found = false;
            while j < n {
                if line[j] == '`' {
                    let mut close_k = 0;
                    while j < n && line[j] == '`' {
                        close_k += 1;
                        j += 1;
                    }
                    if close_k == k {
                        spans.push((start, j));
                        i = j;
                        found = true;
                        break;
                    }
                } else {
                    j += 1;
                }
            }
            if !found {
                // Unclosed code span; treated as normal text
            }
        } else {
            i += 1;
        }
    }

    spans
}

#[derive(Debug, Clone)]
struct DelimRun {
    start: usize,
    len: usize,
    bolds_rem: usize,
    italics_rem: usize,
    can_open: bool,
    can_close: bool,
}

/// Find all matched bold and italic delimiter spans on `line`.
pub fn find_matched_spans(line: &[char], _syntax: &MarkupSyntax) -> Vec<MatchedSpan> {
    let n = line.len();
    if n == 0 {
        return Vec::new();
    }

    // 1. Identify code spans
    let code_spans = find_code_spans(line);
    let in_code = |idx: usize| -> bool {
        code_spans.iter().any(|&(s, e)| idx >= s && idx < e)
    };

    // 2. Identify delimiter runs of '*' outside code spans
    let mut delims: Vec<DelimRun> = Vec::new();
    let mut i = 0;
    while i < n {
        if in_code(i) {
            i += 1;
            continue;
        }

        if line[i] == '*' {
            let start = i;
            while i < n && line[i] == '*' && !in_code(i) {
                i += 1;
            }
            let len = i - start;
            let can_open = i < n && !line[i].is_whitespace() && !in_code(i);
            let can_close = start > 0 && !line[start - 1].is_whitespace() && !in_code(start - 1);

            let bolds = len / 2;
            let italics = len % 2;

            delims.push(DelimRun {
                start,
                len,
                bolds_rem: bolds,
                italics_rem: italics,
                can_open,
                can_close,
            });
        } else {
            i += 1;
        }
    }

    // 3. Match delimiters using stack
    let mut matched = Vec::new();

    for closer_idx in 0..delims.len() {
        if !delims[closer_idx].can_close {
            continue;
        }

        // Try to match bold (2 asterisks) first if available
        while delims[closer_idx].bolds_rem > 0 {
            // Find closest opener that can_open and has bolds_rem > 0
            let mut found_opener = None;
            for opener_idx in (0..closer_idx).rev() {
                if delims[opener_idx].can_open && delims[opener_idx].bolds_rem > 0 {
                    found_opener = Some(opener_idx);
                    break;
                }
            }

            if let Some(op_idx) = found_opener {
                let (openers, closers) = delims.split_at_mut(closer_idx);
                let op = &mut openers[op_idx];
                let cl = &mut closers[0];

                // Determine exact character bounds for the 2 asterisks
                // Opener allocates from the right of its run
                let op_bold_idx = op.len / 2 - op.bolds_rem;
                let open_start = op.start + op_bold_idx * 2;
                let open_end = open_start + 2;

                // Closer allocates from the left of its run
                let cl_bold_idx = cl.len / 2 - cl.bolds_rem;
                let close_start = cl.start + cl_bold_idx * 2;
                let close_end = close_start + 2;

                op.bolds_rem -= 1;
                cl.bolds_rem -= 1;

                matched.push(MatchedSpan {
                    kind: DecorationKind::Bold,
                    open_start,
                    open_end,
                    close_start,
                    close_end,
                });
            } else {
                break;
            }
        }

        // Try to match italic (1 asterisk) if available
        while delims[closer_idx].italics_rem > 0 {
            let mut found_opener = None;
            for opener_idx in (0..closer_idx).rev() {
                if delims[opener_idx].can_open && delims[opener_idx].italics_rem > 0 {
                    found_opener = Some(opener_idx);
                    break;
                }
            }

            if let Some(op_idx) = found_opener {
                let (openers, closers) = delims.split_at_mut(closer_idx);
                let op = &mut openers[op_idx];
                let cl = &mut closers[0];

                // Single asterisk position is at the end of the run
                let open_start = op.start + op.len - 1;
                let open_end = open_start + 1;

                let close_start = cl.start + cl.len - 1;
                let close_end = close_start + 1;

                op.italics_rem -= 1;
                cl.italics_rem -= 1;

                matched.push(MatchedSpan {
                    kind: DecorationKind::Italic,
                    open_start,
                    open_end,
                    close_start,
                    close_end,
                });
            } else {
                break;
            }
        }
    }

    matched
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn parse_plain_text() {
        let text = to_chars("hello world");
        let styles = parse_line_styles(&text, &PANDOC_MARKDOWN);
        assert!(styles.iter().all(|s| !s.bold && !s.italic && !s.is_marker));
    }

    #[test]
    fn parse_bold() {
        let text = to_chars("a **bold** b");
        let styles = parse_line_styles(&text, &PANDOC_MARKDOWN);

        // a (0..2)
        assert!(!styles[0].bold && !styles[0].is_marker);
        // ** (2..4)
        assert!(styles[2].is_marker && !styles[2].bold);
        assert!(styles[3].is_marker && !styles[3].bold);
        // bold (4..8)
        for i in 4..8 {
            assert!(styles[i].bold, "col {} should be bold", i);
            assert!(!styles[i].is_marker);
        }
        // ** (8..10)
        assert!(styles[8].is_marker && !styles[8].bold);
        assert!(styles[9].is_marker && !styles[9].bold);
        // b (10..12)
        assert!(!styles[11].bold && !styles[11].is_marker);
    }

    #[test]
    fn parse_italic() {
        let text = to_chars("a *italic* b");
        let styles = parse_line_styles(&text, &PANDOC_MARKDOWN);

        assert!(styles[2].is_marker && !styles[2].italic);
        for i in 3..9 {
            assert!(styles[i].italic, "col {} should be italic", i);
            assert!(!styles[i].is_marker);
        }
        assert!(styles[9].is_marker && !styles[9].italic);
    }

    #[test]
    fn parse_nested_bold_and_italic() {
        let text = to_chars("**bold *italic* bold**");
        let styles = parse_line_styles(&text, &PANDOC_MARKDOWN);

        // ** (0..2)
        assert!(styles[0].is_marker);
        assert!(styles[1].is_marker);
        // bold (2..7)
        for i in 2..7 {
            assert!(styles[i].bold && !styles[i].italic);
        }
        // * (7..8)
        assert!(styles[7].is_marker);
        // italic (8..14)
        for i in 8..14 {
            assert!(styles[i].bold && styles[i].italic, "col {} should be bold and italic", i);
        }
        // * (14..15)
        assert!(styles[14].is_marker);
        // bold (15..20)
        for i in 15..20 {
            assert!(styles[i].bold && !styles[i].italic);
        }
        // ** (20..22)
        assert!(styles[20].is_marker);
        assert!(styles[21].is_marker);
    }

    #[test]
    fn unclosed_markers_remain_plain() {
        let text = to_chars("**unclosed bold text");
        let styles = parse_line_styles(&text, &PANDOC_MARKDOWN);
        assert!(styles.iter().all(|s| !s.bold && !s.italic && !s.is_marker));

        let text2 = to_chars("*unclosed italic");
        let styles2 = parse_line_styles(&text2, &PANDOC_MARKDOWN);
        assert!(styles2.iter().all(|s| !s.bold && !s.italic && !s.is_marker));
    }

    #[test]
    fn code_span_suppresses_decoration() {
        let text = to_chars("`code with **stars**` **real bold**");
        let styles = parse_line_styles(&text, &PANDOC_MARKDOWN);

        // inside code span (0..21)
        for i in 0..21 {
            assert!(!styles[i].bold, "col {} should not be bold", i);
            assert!(!styles[i].is_marker, "col {} should not be marker", i);
        }

        // real bold (22..35)
        assert!(styles[22].is_marker);
        assert!(styles[23].is_marker);
        for i in 24..33 {
            assert!(styles[i].bold);
        }
        assert!(styles[33].is_marker);
        assert!(styles[34].is_marker);
    }

    #[test]
    fn whitespace_rules_prevent_false_matches() {
        let text = to_chars("** not bold** and **not bold **");
        let styles = parse_line_styles(&text, &PANDOC_MARKDOWN);
        assert!(styles.iter().all(|s| !s.bold && !s.is_marker));
    }

    #[test]
    fn find_enclosing_bold_span() {
        let text = to_chars("foo **bar** baz");
        let span = find_enclosing_decoration_span(&text, 6, DecorationKind::Bold, &PANDOC_MARKDOWN);
        assert!(span.is_some());
        let s = span.unwrap();
        assert_eq!((s.open_start, s.open_end), (4, 6));
        assert_eq!((s.close_start, s.close_end), (9, 11));

        // Outside span
        assert!(find_enclosing_decoration_span(&text, 2, DecorationKind::Bold, &PANDOC_MARKDOWN).is_none());
        assert!(find_enclosing_decoration_span(&text, 12, DecorationKind::Bold, &PANDOC_MARKDOWN).is_none());
    }

    #[test]
    fn find_enclosing_empty_pair() {
        let text = to_chars("foo **** baz");
        let span = find_enclosing_decoration_span(&text, 6, DecorationKind::Bold, &PANDOC_MARKDOWN);
        assert!(span.is_some());
        let s = span.unwrap();
        assert_eq!((s.open_start, s.open_end), (4, 6));
        assert_eq!((s.close_start, s.close_end), (6, 8));
    }
}
