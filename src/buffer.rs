use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A (line, char-column) position within a buffer.
pub type Pos = (usize, usize);

/// Line ending style detected on load, preserved on save.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

/// The in-memory text of one file, stored as lines of chars so that
/// editing and cursor math work in Unicode scalar values rather than
/// raw bytes (important for Japanese text).
pub struct Buffer {
    pub lines: Vec<Vec<char>>,
    pub filename: Option<PathBuf>,
    pub dirty: bool,
    pub line_ending: LineEnding,
}

impl Buffer {
    pub fn new() -> Self {
        Buffer {
            lines: vec![Vec::new()],
            filename: None,
            dirty: false,
            line_ending: LineEnding::Lf,
        }
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(path)?;
        let line_ending = if content.contains("\r\n") {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        };
        let normalized = content.replace("\r\n", "\n");
        let mut lines: Vec<Vec<char>> = normalized.split('\n').map(|s| s.chars().collect()).collect();
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        Ok(Buffer {
            lines,
            filename: Some(path.to_path_buf()),
            dirty: false,
            line_ending,
        })
    }

    pub fn save_as(&mut self, path: &Path) -> io::Result<()> {
        self.write_to(path)?;
        self.filename = Some(path.to_path_buf());
        self.dirty = false;
        Ok(())
    }

    pub fn save(&mut self) -> io::Result<PathBuf> {
        let path = self
            .filename
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no filename set"))?;
        self.write_to(&path)?;
        self.dirty = false;
        Ok(path)
    }

    fn write_to(&self, path: &Path) -> io::Result<()> {
        self.write_block(path, &self.lines)
    }

    pub fn line_len(&self, line: usize) -> usize {
        self.lines[line].len()
    }

    pub fn num_lines(&self) -> usize {
        self.lines.len()
    }

    /// Write an arbitrary set of lines (e.g. a marked block) to `path`,
    /// independent of this buffer's own filename.
    pub fn write_block(&self, path: &Path, lines: &[Vec<char>]) -> io::Result<()> {
        let sep = match self.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        };
        fs::write(path, lines_to_text(lines).replace('\n', sep))
    }

    /// Search forward from `from` (inclusive) for `needle`, wrapping around
    /// to the start of the document if not found before the end. Returns
    /// the match's (start, end) and whether the search had to wrap. When
    /// `allow_wrap` is false, only the forward pass runs — used by
    /// "replace all" so it never re-visits a match earlier in the
    /// document that was already replaced or explicitly skipped.
    pub fn find_from(
        &self,
        from: Pos,
        needle: &[char],
        case_sensitive: bool,
        allow_wrap: bool,
    ) -> Option<(Pos, Pos, bool)> {
        if needle.is_empty() {
            return None;
        }
        let total = self.num_lines();
        for l in from.0..total {
            let start_col = if l == from.0 { from.1 } else { 0 };
            if let Some(pos) = find_in_line(&self.lines[l], needle, start_col, case_sensitive) {
                return Some(((l, pos), (l, pos + needle.len()), false));
            }
        }
        if !allow_wrap {
            return None;
        }
        for l in 0..=from.0.min(total.saturating_sub(1)) {
            if let Some(pos) = find_in_line(&self.lines[l], needle, 0, case_sensitive) {
                return Some(((l, pos), (l, pos + needle.len()), true));
            }
        }
        None
    }

    pub fn display_name(&self) -> String {
        match &self.filename {
            Some(p) => p.display().to_string(),
            None => "UNTITLED.TXT".to_string(),
        }
    }

    /// Clamp a (line, col) pair to valid bounds within the buffer.
    pub fn clamp(&self, pos: Pos) -> Pos {
        let line = pos.0.min(self.num_lines() - 1);
        let col = pos.1.min(self.line_len(line));
        (line, col)
    }

    /// Extract the text between start and end (start <= end) as a list of
    /// line fragments, without modifying the buffer.
    pub fn extract_range(&self, start: Pos, end: Pos) -> Vec<Vec<char>> {
        let (sl, sc) = start;
        let (el, ec) = end;
        if sl == el {
            return vec![self.lines[sl][sc..ec].to_vec()];
        }
        let mut result = Vec::with_capacity(el - sl + 1);
        result.push(self.lines[sl][sc..].to_vec());
        for l in (sl + 1)..el {
            result.push(self.lines[l].clone());
        }
        result.push(self.lines[el][..ec].to_vec());
        result
    }

    /// Delete the text between start and end (start <= end) in place.
    pub fn delete_range(&mut self, start: Pos, end: Pos) {
        let (sl, sc) = start;
        let (el, ec) = end;
        if sl == el {
            self.lines[sl].drain(sc..ec);
        } else {
            let tail: Vec<char> = self.lines[el][ec..].to_vec();
            self.lines[sl].truncate(sc);
            self.lines[sl].extend(tail);
            self.lines.drain(sl + 1..=el);
        }
        self.dirty = true;
    }

    /// Insert (possibly multi-line) text at pos, splicing it into the
    /// existing line structure. Returns the cursor position at the end
    /// of the inserted text.
    pub fn insert_lines_at(&mut self, pos: Pos, text: &[Vec<char>]) -> Pos {
        let (l, c) = pos;
        if text.is_empty() {
            return pos;
        }
        self.dirty = true;
        if text.len() == 1 {
            self.lines[l].splice(c..c, text[0].iter().cloned());
            return (l, c + text[0].len());
        }
        let tail: Vec<char> = self.lines[l][c..].to_vec();
        self.lines[l].truncate(c);
        self.lines[l].extend(text[0].iter().cloned());

        let mut new_lines: Vec<Vec<char>> = Vec::with_capacity(text.len() - 1);
        for line in &text[1..text.len() - 1] {
            new_lines.push(line.clone());
        }
        let mut last = text[text.len() - 1].clone();
        let last_len = last.len();
        last.extend(tail);
        new_lines.push(last);

        for (i, nl) in new_lines.into_iter().enumerate() {
            self.lines.insert(l + 1 + i, nl);
        }
        (l + text.len() - 1, last_len)
    }
}

fn find_in_line(line: &[char], needle: &[char], from_col: usize, case_sensitive: bool) -> Option<usize> {
    let mut i = from_col;
    while i + needle.len() <= line.len() {
        let mut matched = true;
        for j in 0..needle.len() {
            let a = line[i + j];
            let b = needle[j];
            let eq = if case_sensitive {
                a == b
            } else {
                a.to_lowercase().eq(b.to_lowercase())
            };
            if !eq {
                matched = false;
                break;
            }
        }
        if matched {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub fn text_to_lines(text: &str) -> Vec<Vec<char>> {
    text.replace("\r\n", "\n")
        .split('\n')
        .map(|s| s.chars().collect())
        .collect()
}

pub fn lines_to_text(lines: &[Vec<char>]) -> String {
    lines
        .iter()
        .map(|l| l.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(text: &str) -> Buffer {
        Buffer {
            lines: text_to_lines(text),
            filename: None,
            dirty: false,
            line_ending: LineEnding::Lf,
        }
    }

    #[test]
    fn extract_range_single_line() {
        let b = buf("hello world");
        let out = b.extract_range((0, 6), (0, 11));
        assert_eq!(lines_to_text(&out), "world");
    }

    #[test]
    fn extract_range_multi_line() {
        let b = buf("abc\ndefg\nhi");
        let out = b.extract_range((0, 1), (2, 1));
        assert_eq!(lines_to_text(&out), "bc\ndefg\nh");
    }

    #[test]
    fn delete_range_multi_line_joins() {
        let mut b = buf("abc\ndefg\nhi");
        b.delete_range((0, 1), (2, 1));
        assert_eq!(lines_to_text(&b.lines), "ai");
    }

    #[test]
    fn insert_lines_at_single_line() {
        let mut b = buf("hello world");
        let end = b.insert_lines_at((0, 5), &text_to_lines(", cruel"));
        assert_eq!(lines_to_text(&b.lines), "hello, cruel world");
        assert_eq!(end, (0, 12));
    }

    #[test]
    fn insert_lines_at_multi_line_splices() {
        let mut b = buf("ai");
        let end = b.insert_lines_at((0, 1), &text_to_lines("bc\ndefg\nh"));
        assert_eq!(lines_to_text(&b.lines), "abc\ndefg\nhi");
        assert_eq!(end, (2, 1));
    }

    #[test]
    fn roundtrip_extract_delete_insert_is_identity() {
        let original = "line one\nline two\nline three\n";
        let mut b = buf(original);
        let extracted = b.extract_range((0, 5), (2, 4));
        b.delete_range((0, 5), (2, 4));
        let end = b.insert_lines_at((0, 5), &extracted);
        assert_eq!(lines_to_text(&b.lines), original);
        assert_eq!(end, (2, 4));
    }

    #[test]
    fn find_from_wraps_around() {
        let b = buf("needle here\nsecond needle\nlast line");
        // start searching from just past the first match; should find the
        // second occurrence, then wrap around and find the first again.
        let (s1, e1, w1) = b
            .find_from((0, 1), &['n', 'e', 'e', 'd', 'l', 'e'], false, true)
            .unwrap();
        assert_eq!((s1, e1, w1), ((1, 7), (1, 13), false));
        let (s2, _e2, w2) = b
            .find_from((1, 8), &['n', 'e', 'e', 'd', 'l', 'e'], false, true)
            .unwrap();
        assert_eq!(s2, (0, 0));
        assert!(w2);
    }

    #[test]
    fn find_from_no_wrap_stops_at_end_of_document() {
        let b = buf("needle here\nsecond needle\nlast line");
        // past the second occurrence: with wrap disabled this must find
        // nothing, even though an earlier occurrence exists in the doc.
        let found = b.find_from((1, 8), &['n', 'e', 'e', 'd', 'l', 'e'], false, false);
        assert_eq!(found, None);
    }

    #[test]
    fn find_from_case_insensitive() {
        let b = buf("Hello World");
        let found = b.find_from((0, 0), &['w', 'o', 'r', 'l', 'd'], false, true);
        assert_eq!(found, Some(((0, 6), (0, 11), false)));
        let not_found = b.find_from((0, 0), &['w', 'o', 'r', 'l', 'd'], true, true);
        assert_eq!(not_found, None);
    }

    #[test]
    fn save_and_load_roundtrip_preserves_crlf() {
        let dir = std::env::temp_dir().join(format!("mustang-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("crlf.txt");
        std::fs::write(&path, "one\r\ntwo\r\nthree").unwrap();

        let mut b = Buffer::load(&path).unwrap();
        assert!(matches!(b.line_ending, LineEnding::CrLf));
        assert_eq!(b.lines.len(), 3);

        let save_path = dir.join("out.txt");
        b.save_as(&save_path).unwrap();
        let raw = std::fs::read_to_string(&save_path).unwrap();
        assert_eq!(raw, "one\r\ntwo\r\nthree");

        std::fs::remove_dir_all(&dir).ok();
    }
}
