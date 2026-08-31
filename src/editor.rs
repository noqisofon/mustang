use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::buffer::{self, Buffer};
use crate::keymap::{self, Command, PrefixState};
use crate::util::{self, TAB_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Space,
    Word,
    Punct,
}

#[derive(Debug, Clone, Copy)]
pub enum SaveThen {
    Stay,
    Exit,
}

#[derive(Debug, Clone, Copy)]
pub enum PromptKind {
    Find,
    ReplaceSearch,
    ReplaceWith,
    SaveAs(SaveThen),
    WriteBlock,
    ReadFile,
    GoToLine,
    ConfirmQuit,
}

struct ReplaceState {
    search: String,
    replace: String,
    match_start: (usize, usize),
    match_end: (usize, usize),
}

pub struct Editor {
    pub buffer: Buffer,
    pub cur_line: usize,
    pub cur_col: usize,
    pub top_line: usize,
    pub left_col: usize,
    pub insert_mode: bool,
    pub mark_start: Option<(usize, usize)>,
    pub mark_end: Option<(usize, usize)>,
    pub block_visible: bool,
    pub message: String,
    pub quit: bool,
    pub menu_visible: bool,
    pub help_visible: bool,
    pub prefix: PrefixState,
    pub prompt: Option<PromptKind>,
    pub prompt_input: String,
    pub prompt_label: String,
    pub text_rows: usize,
    pub text_cols: usize,

    pending_replace_search: Option<String>,
    last_search: Option<String>,
    last_match_end: Option<(usize, usize)>,
    replace_state: Option<ReplaceState>,
}

impl Editor {
    pub fn new(buffer: Buffer) -> Self {
        Editor {
            buffer,
            cur_line: 0,
            cur_col: 0,
            top_line: 0,
            left_col: 0,
            insert_mode: true,
            mark_start: None,
            mark_end: None,
            block_visible: false,
            message: "mustang -- WordStar 3.3 style editor. ^J for help.".to_string(),
            quit: false,
            menu_visible: true,
            help_visible: false,
            prefix: PrefixState::None,
            prompt: None,
            prompt_input: String::new(),
            prompt_label: String::new(),
            text_rows: 1,
            text_cols: 1,
            pending_replace_search: None,
            last_search: None,
            last_match_end: None,
            replace_state: None,
        }
    }

    // ---- top-level key handling -----------------------------------

    pub fn handle_key(&mut self, ev: KeyEvent) {
        if self.help_visible {
            self.help_visible = false;
            return;
        }
        if self.prompt.is_some() {
            self.handle_prompt_key(ev);
            return;
        }
        if self.replace_state.is_some() {
            self.handle_replace_confirm_key(ev);
            return;
        }
        let cmd = keymap::translate(ev, &mut self.prefix);
        self.execute(cmd);
    }

    fn execute(&mut self, cmd: Command) {
        if cmd != Command::None {
            self.message.clear();
        }
        match cmd {
            Command::None => {}
            Command::Beep => self.message = "Not a WordStar command".to_string(),
            Command::CancelPrefix => self.message = "Cancelled".to_string(),

            Command::MoveUp => self.move_up(),
            Command::MoveDown => self.move_down(),
            Command::MoveLeft => self.move_left(),
            Command::MoveRight => self.move_right(),
            Command::WordLeft => self.word_left(),
            Command::WordRight => self.word_right(),
            Command::LineStart => self.cur_col = 0,
            Command::LineEnd => self.cur_col = self.buffer.line_len(self.cur_line),
            Command::ScreenTop => self.cur_line = self.top_line.min(self.buffer.num_lines() - 1),
            Command::ScreenBottom => {
                self.cur_line = (self.top_line + self.text_rows.saturating_sub(1))
                    .min(self.buffer.num_lines() - 1)
            }
            Command::DocStart => {
                self.cur_line = 0;
                self.cur_col = 0;
            }
            Command::DocEnd => {
                self.cur_line = self.buffer.num_lines() - 1;
                self.cur_col = self.buffer.line_len(self.cur_line);
            }
            Command::PageUp => self.page_up(),
            Command::PageDown => self.page_down(),
            Command::GoToBlockBegin => {
                if let Some(p) = self.mark_start {
                    self.cur_line = p.0;
                    self.cur_col = p.1;
                } else {
                    self.message = "No block marked".to_string();
                }
            }
            Command::GoToBlockEnd => {
                if let Some(p) = self.mark_end {
                    self.cur_line = p.0;
                    self.cur_col = p.1;
                } else {
                    self.message = "No block marked".to_string();
                }
            }
            Command::GoToLinePrompt => self.start_prompt(PromptKind::GoToLine, "Go to line:"),

            Command::InsertChar(c) => self.insert_char(c),
            Command::Enter => self.newline(),
            Command::Tab => self.insert_char('\t'),
            Command::OpenLine => self.open_line(),
            Command::DeleteCharRight => self.delete_char_right(),
            Command::DeleteCharLeft => self.delete_char_left(),
            Command::DeleteWordRight => self.delete_word_right(),
            Command::DeleteLine => self.delete_line(),
            Command::DeleteToEol => self.delete_to_eol(),
            Command::ToggleInsertMode => self.insert_mode = !self.insert_mode,

            Command::MarkBegin => {
                self.mark_start = Some((self.cur_line, self.cur_col));
                self.block_visible = true;
                self.message = "Block begin".to_string();
            }
            Command::MarkEnd => {
                self.mark_end = Some((self.cur_line, self.cur_col));
                self.block_visible = true;
                self.message = "Block end".to_string();
            }
            Command::CopyBlock => self.copy_block(),
            Command::MoveBlock => self.move_block(),
            Command::DeleteBlock => self.delete_block(),
            Command::HideBlock => {
                if self.mark_start.is_none() && self.mark_end.is_none() {
                    self.message = "No block marked".to_string();
                } else {
                    self.block_visible = !self.block_visible;
                }
            }
            Command::WriteBlockPrompt => self.start_prompt(PromptKind::WriteBlock, "Write block to file:"),
            Command::ReadFilePrompt => self.start_prompt(PromptKind::ReadFile, "Read file into text:"),

            Command::Save => self.do_save(SaveThen::Stay),
            Command::SaveDone => self.do_save(SaveThen::Stay),
            Command::SaveExitPrompt => self.do_save(SaveThen::Exit),
            Command::QuitPrompt => {
                if self.buffer.dirty {
                    self.start_prompt(PromptKind::ConfirmQuit, "Abandon changes since last save? (Y/N):");
                } else {
                    self.quit = true;
                }
            }

            Command::FindPrompt => self.start_prompt(PromptKind::Find, "Find:"),
            Command::ReplacePrompt => self.start_prompt(PromptKind::ReplaceSearch, "Find:"),
            Command::RepeatFind => self.repeat_find(),

            Command::ToggleMenu => self.menu_visible = !self.menu_visible,
            Command::Help => self.help_visible = true,
        }
        self.clamp_cursor();
    }

    fn clamp_cursor(&mut self) {
        let pos = self.buffer.clamp((self.cur_line, self.cur_col));
        self.cur_line = pos.0;
        self.cur_col = pos.1;
    }

    // ---- movement ---------------------------------------------------

    pub fn move_up(&mut self) {
        if self.cur_line > 0 {
            self.cur_line -= 1;
            self.cur_col = self.cur_col.min(self.buffer.line_len(self.cur_line));
        }
    }

    pub fn move_down(&mut self) {
        if self.cur_line + 1 < self.buffer.num_lines() {
            self.cur_line += 1;
            self.cur_col = self.cur_col.min(self.buffer.line_len(self.cur_line));
        }
    }

    pub fn move_left(&mut self) {
        if self.cur_col > 0 {
            self.cur_col -= 1;
        } else if self.cur_line > 0 {
            self.cur_line -= 1;
            self.cur_col = self.buffer.line_len(self.cur_line);
        }
    }

    pub fn move_right(&mut self) {
        if self.cur_col < self.buffer.line_len(self.cur_line) {
            self.cur_col += 1;
        } else if self.cur_line + 1 < self.buffer.num_lines() {
            self.cur_line += 1;
            self.cur_col = 0;
        }
    }

    fn page_up(&mut self) {
        let rows = self.text_rows.max(1);
        self.cur_line = self.cur_line.saturating_sub(rows);
        self.cur_col = self.cur_col.min(self.buffer.line_len(self.cur_line));
        self.top_line = self.top_line.saturating_sub(rows);
    }

    fn page_down(&mut self) {
        let rows = self.text_rows.max(1);
        let last = self.buffer.num_lines() - 1;
        self.cur_line = (self.cur_line + rows).min(last);
        self.cur_col = self.cur_col.min(self.buffer.line_len(self.cur_line));
        self.top_line = (self.top_line + rows).min(last);
    }

    fn char_class_at(&self, pos: (usize, usize)) -> CharClass {
        let (l, c) = pos;
        if c >= self.buffer.line_len(l) {
            CharClass::Space
        } else {
            let ch = self.buffer.lines[l][c];
            if ch.is_whitespace() {
                CharClass::Space
            } else if ch.is_alphanumeric() || ch == '_' {
                CharClass::Word
            } else {
                CharClass::Punct
            }
        }
    }

    fn advance_pos(&self, pos: (usize, usize)) -> Option<(usize, usize)> {
        let (l, c) = pos;
        if c < self.buffer.line_len(l) {
            Some((l, c + 1))
        } else if l + 1 < self.buffer.num_lines() {
            Some((l + 1, 0))
        } else {
            None
        }
    }

    fn retreat_pos(&self, pos: (usize, usize)) -> Option<(usize, usize)> {
        let (l, c) = pos;
        if c > 0 {
            Some((l, c - 1))
        } else if l > 0 {
            Some((l - 1, self.buffer.line_len(l - 1)))
        } else {
            None
        }
    }

    pub fn word_right(&mut self) {
        let mut pos = (self.cur_line, self.cur_col);
        let start_class = self.char_class_at(pos);
        if start_class != CharClass::Space {
            while self.char_class_at(pos) == start_class {
                match self.advance_pos(pos) {
                    Some(p) => pos = p,
                    None => break,
                }
            }
        }
        while self.char_class_at(pos) == CharClass::Space {
            match self.advance_pos(pos) {
                Some(p) => pos = p,
                None => break,
            }
        }
        self.cur_line = pos.0;
        self.cur_col = pos.1;
    }

    pub fn word_left(&mut self) {
        let mut pos = (self.cur_line, self.cur_col);
        loop {
            match self.retreat_pos(pos) {
                Some(prev) if self.char_class_at(prev) == CharClass::Space => pos = prev,
                Some(prev) => {
                    pos = prev;
                    break;
                }
                None => {
                    self.cur_line = pos.0;
                    self.cur_col = pos.1;
                    return;
                }
            }
        }
        let class = self.char_class_at(pos);
        loop {
            match self.retreat_pos(pos) {
                Some(prev) if self.char_class_at(prev) == class => pos = prev,
                _ => break,
            }
        }
        self.cur_line = pos.0;
        self.cur_col = pos.1;
    }

    // ---- editing ------------------------------------------------------

    pub fn insert_char(&mut self, ch: char) {
        let line = &mut self.buffer.lines[self.cur_line];
        if self.insert_mode || self.cur_col >= line.len() {
            line.insert(self.cur_col, ch);
        } else {
            line[self.cur_col] = ch;
        }
        self.cur_col += 1;
        self.buffer.dirty = true;
    }

    pub fn newline(&mut self) {
        let line = &mut self.buffer.lines[self.cur_line];
        let rest = line.split_off(self.cur_col.min(line.len()));
        self.buffer.lines.insert(self.cur_line + 1, rest);
        self.cur_line += 1;
        self.cur_col = 0;
        self.buffer.dirty = true;
    }

    pub fn open_line(&mut self) {
        let line = &mut self.buffer.lines[self.cur_line];
        let rest = line.split_off(self.cur_col.min(line.len()));
        self.buffer.lines.insert(self.cur_line + 1, rest);
        self.buffer.dirty = true;
    }

    pub fn delete_char_right(&mut self) {
        let len = self.buffer.line_len(self.cur_line);
        if self.cur_col < len {
            self.buffer.lines[self.cur_line].remove(self.cur_col);
            self.buffer.dirty = true;
        } else if self.cur_line + 1 < self.buffer.num_lines() {
            let next = self.buffer.lines.remove(self.cur_line + 1);
            self.buffer.lines[self.cur_line].extend(next);
            self.buffer.dirty = true;
        }
    }

    pub fn delete_char_left(&mut self) {
        if self.cur_col > 0 {
            self.buffer.lines[self.cur_line].remove(self.cur_col - 1);
            self.cur_col -= 1;
            self.buffer.dirty = true;
        } else if self.cur_line > 0 {
            let cur = self.buffer.lines.remove(self.cur_line);
            self.cur_line -= 1;
            self.cur_col = self.buffer.line_len(self.cur_line);
            self.buffer.lines[self.cur_line].extend(cur);
            self.buffer.dirty = true;
        }
    }

    pub fn delete_word_right(&mut self) {
        let start = (self.cur_line, self.cur_col);
        self.word_right();
        let end = (self.cur_line, self.cur_col);
        self.cur_line = start.0;
        self.cur_col = start.1;
        if end != start {
            self.buffer.delete_range(start, end);
        }
    }

    pub fn delete_line(&mut self) {
        if self.buffer.num_lines() == 1 {
            self.buffer.lines[0].clear();
        } else {
            self.buffer.lines.remove(self.cur_line);
            if self.cur_line >= self.buffer.num_lines() {
                self.cur_line = self.buffer.num_lines() - 1;
            }
        }
        self.cur_col = 0;
        self.buffer.dirty = true;
        self.clear_mark();
    }

    pub fn delete_to_eol(&mut self) {
        self.buffer.lines[self.cur_line].truncate(self.cur_col);
        self.buffer.dirty = true;
    }

    // ---- block operations ----------------------------------------------

    pub fn normalized_marks(&self) -> Option<((usize, usize), (usize, usize))> {
        match (self.mark_start, self.mark_end) {
            (Some(a), Some(b)) => Some(if a <= b { (a, b) } else { (b, a) }),
            _ => None,
        }
    }

    fn clear_mark(&mut self) {
        self.mark_start = None;
        self.mark_end = None;
        self.block_visible = false;
    }

    fn copy_block(&mut self) {
        let Some((s, e)) = self.normalized_marks() else {
            self.message = "No block marked".to_string();
            return;
        };
        let text = self.buffer.extract_range(s, e);
        let pos = (self.cur_line, self.cur_col);
        let new_pos = self.buffer.insert_lines_at(pos, &text);
        self.cur_line = new_pos.0;
        self.cur_col = new_pos.1;
        self.message = "Block copied".to_string();
    }

    fn move_block(&mut self) {
        let Some((s, e)) = self.normalized_marks() else {
            self.message = "No block marked".to_string();
            return;
        };
        let pos = (self.cur_line, self.cur_col);
        if pos >= s && pos <= e {
            self.message = "Cannot move a block into itself".to_string();
            return;
        }
        let text = self.buffer.extract_range(s, e);
        self.buffer.delete_range(s, e);
        let adj_pos = if pos < s {
            pos
        } else {
            let lines_removed = e.0 - s.0;
            if pos.0 == e.0 {
                (s.0, s.1 + pos.1.saturating_sub(e.1))
            } else {
                (pos.0 - lines_removed, pos.1)
            }
        };
        let new_end = self.buffer.insert_lines_at(adj_pos, &text);
        self.cur_line = new_end.0;
        self.cur_col = new_end.1;
        self.mark_start = Some(adj_pos);
        self.mark_end = Some(new_end);
        self.message = "Block moved".to_string();
    }

    fn delete_block(&mut self) {
        let Some((s, e)) = self.normalized_marks() else {
            self.message = "No block marked".to_string();
            return;
        };
        self.buffer.delete_range(s, e);
        self.cur_line = s.0;
        self.cur_col = s.1;
        self.clear_mark();
        self.message = "Block deleted".to_string();
    }

    // ---- search / replace -----------------------------------------------

    fn do_find(&mut self, needle_str: &str) {
        let needle: Vec<char> = needle_str.chars().collect();
        let from = (self.cur_line, self.cur_col);
        match self.buffer.find_from(from, &needle, false, true) {
            Some((s, e, wrapped)) => {
                self.cur_line = s.0;
                self.cur_col = s.1;
                self.mark_start = Some(s);
                self.mark_end = Some(e);
                self.block_visible = true;
                self.last_match_end = Some(e);
                self.message = if wrapped {
                    "Found (search wrapped)".to_string()
                } else {
                    "Found".to_string()
                };
            }
            None => self.message = format!("\"{}\" not found", needle_str),
        }
    }

    fn repeat_find(&mut self) {
        let Some(text) = self.last_search.clone() else {
            self.message = "No previous Find/Replace".to_string();
            return;
        };
        let needle: Vec<char> = text.chars().collect();
        let from = self.last_match_end.unwrap_or((self.cur_line, self.cur_col));
        match self.buffer.find_from(from, &needle, false, true) {
            Some((s, e, wrapped)) => {
                self.cur_line = s.0;
                self.cur_col = s.1;
                self.mark_start = Some(s);
                self.mark_end = Some(e);
                self.block_visible = true;
                self.last_match_end = Some(e);
                self.message = if wrapped {
                    "Found (search wrapped)".to_string()
                } else {
                    "Found".to_string()
                };
            }
            None => self.message = format!("\"{}\" not found", text),
        }
    }

    fn do_replace_at(&mut self, s: (usize, usize), e: (usize, usize), replace: &str) -> (usize, usize) {
        self.buffer.delete_range(s, e);
        let repl_lines = buffer::text_to_lines(replace);
        self.buffer.insert_lines_at(s, &repl_lines)
    }

    fn begin_replace(&mut self, search: String, replace: String) {
        if search.is_empty() {
            self.message = "Replace cancelled (empty search)".to_string();
            return;
        }
        self.last_search = Some(search.clone());
        self.try_next_replace_match(search, replace, false);
    }

    fn try_next_replace_match(&mut self, search: String, replace: String, replace_all: bool) {
        let needle: Vec<char> = search.chars().collect();
        let mut guard = 0usize;
        loop {
            let from = (self.cur_line, self.cur_col);
            match self.buffer.find_from(from, &needle, false, false) {
                Some((s, e, wrapped)) => {
                    self.cur_line = s.0;
                    self.cur_col = s.1;
                    self.mark_start = Some(s);
                    self.mark_end = Some(e);
                    self.block_visible = true;
                    self.last_match_end = Some(e);
                    if replace_all {
                        guard += 1;
                        if guard > 200_000 {
                            self.message = "Too many replacements; stopped.".to_string();
                            self.replace_state = None;
                            return;
                        }
                        let end_pos = self.do_replace_at(s, e, &replace);
                        self.cur_line = end_pos.0;
                        self.cur_col = end_pos.1;
                        continue;
                    } else {
                        self.replace_state = Some(ReplaceState {
                            search,
                            replace,
                            match_start: s,
                            match_end: e,
                        });
                        self.message = format!(
                            "Replace this occurrence? Y)es N)o A)ll Esc)stop{}",
                            if wrapped { " [wrapped]" } else { "" }
                        );
                        return;
                    }
                }
                None => {
                    self.replace_state = None;
                    self.clear_mark();
                    self.message = if replace_all {
                        "Replace complete.".to_string()
                    } else {
                        "No occurrences found.".to_string()
                    };
                    return;
                }
            }
        }
    }

    fn handle_replace_confirm_key(&mut self, ev: KeyEvent) {
        let Some(state) = self.replace_state.take() else {
            return;
        };
        match ev.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let end_pos = self.do_replace_at(state.match_start, state.match_end, &state.replace);
                self.cur_line = end_pos.0;
                self.cur_col = end_pos.1;
                self.try_next_replace_match(state.search, state.replace, false);
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.cur_line = state.match_end.0;
                self.cur_col = state.match_end.1;
                self.try_next_replace_match(state.search, state.replace, false);
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.cur_line = state.match_start.0;
                self.cur_col = state.match_start.1;
                self.try_next_replace_match(state.search, state.replace, true);
            }
            _ => {
                self.clear_mark();
                self.message = "Replace stopped.".to_string();
            }
        }
    }

    // ---- prompts -----------------------------------------------------

    fn start_prompt(&mut self, kind: PromptKind, label: &str) {
        self.prompt = Some(kind);
        self.prompt_input.clear();
        self.prompt_label = label.to_string();
    }

    fn handle_prompt_key(&mut self, ev: KeyEvent) {
        let Some(kind) = self.prompt else { return };
        match ev.code {
            KeyCode::Esc => {
                self.prompt = None;
                self.prompt_input.clear();
                self.message = "Cancelled".to_string();
            }
            KeyCode::Enter => {
                let input = std::mem::take(&mut self.prompt_input);
                self.prompt = None;
                self.submit_prompt(kind, input);
            }
            KeyCode::Backspace => {
                self.prompt_input.pop();
            }
            KeyCode::Char(c) => {
                if ev.modifiers.contains(KeyModifiers::CONTROL) {
                    if c == 'h' || c == 'H' {
                        self.prompt_input.pop();
                    }
                } else {
                    self.prompt_input.push(c);
                }
            }
            _ => {}
        }
    }

    fn submit_prompt(&mut self, kind: PromptKind, input: String) {
        match kind {
            PromptKind::Find => {
                if input.is_empty() {
                    self.message = "Find cancelled (empty)".to_string();
                    return;
                }
                self.last_search = Some(input.clone());
                self.do_find(&input);
            }
            PromptKind::ReplaceSearch => {
                if input.is_empty() {
                    self.message = "Replace cancelled (empty search)".to_string();
                    return;
                }
                self.pending_replace_search = Some(input.clone());
                self.start_prompt(PromptKind::ReplaceWith, &format!("Replace \"{}\" with:", input));
            }
            PromptKind::ReplaceWith => {
                let search = self.pending_replace_search.take().unwrap_or_default();
                self.begin_replace(search, input);
            }
            PromptKind::SaveAs(then) => {
                if input.is_empty() {
                    self.message = "Save cancelled".to_string();
                    return;
                }
                let path = PathBuf::from(input);
                match self.buffer.save_as(&path) {
                    Ok(_) => {
                        self.message = format!("Saved {}", self.buffer.display_name());
                        if let SaveThen::Exit = then {
                            self.quit = true;
                        }
                    }
                    Err(e) => self.message = format!("Save failed: {}", e),
                }
            }
            PromptKind::WriteBlock => {
                if input.is_empty() {
                    self.message = "Cancelled".to_string();
                    return;
                }
                match self.normalized_marks() {
                    Some((s, e)) => {
                        let text = self.buffer.extract_range(s, e);
                        match self.buffer.write_block(Path::new(&input), &text) {
                            Ok(_) => self.message = format!("Block written to {}", input),
                            Err(err) => self.message = format!("Write failed: {}", err),
                        }
                    }
                    None => self.message = "No block marked".to_string(),
                }
            }
            PromptKind::ReadFile => {
                if input.is_empty() {
                    self.message = "Cancelled".to_string();
                    return;
                }
                match fs::read_to_string(&input) {
                    Ok(content) => {
                        let lines = buffer::text_to_lines(&content);
                        let pos = (self.cur_line, self.cur_col);
                        let new_pos = self.buffer.insert_lines_at(pos, &lines);
                        self.cur_line = new_pos.0;
                        self.cur_col = new_pos.1;
                        self.buffer.dirty = true;
                        self.message = format!("Read {}", input);
                    }
                    Err(e) => self.message = format!("Read failed: {}", e),
                }
            }
            PromptKind::GoToLine => match input.trim().parse::<usize>() {
                Ok(n) if n >= 1 => {
                    self.cur_line = (n - 1).min(self.buffer.num_lines() - 1);
                    self.cur_col = 0;
                }
                _ => self.message = "Invalid line number".to_string(),
            },
            PromptKind::ConfirmQuit => {
                if input.eq_ignore_ascii_case("y") {
                    self.quit = true;
                } else {
                    self.message = "Quit cancelled".to_string();
                }
            }
        }
    }

    // ---- file ops -----------------------------------------------------

    fn do_save(&mut self, then: SaveThen) {
        if self.buffer.filename.is_some() {
            match self.buffer.save() {
                Ok(p) => {
                    self.message = format!("Saved {}", p.display());
                    if let SaveThen::Exit = then {
                        self.quit = true;
                    }
                }
                Err(e) => self.message = format!("Save failed: {}", e),
            }
        } else {
            self.start_prompt(PromptKind::SaveAs(then), "Save as:");
        }
    }

    // ---- viewport -----------------------------------------------------

    pub fn set_viewport(&mut self, rows: usize, cols: usize) {
        self.text_rows = rows;
        self.text_cols = cols;
    }

    fn display_col(&self, line: usize, col: usize) -> usize {
        util::display_width(&self.buffer.lines[line], col, TAB_SIZE)
    }

    pub fn ensure_visible(&mut self) {
        let rows = self.text_rows.max(1);
        if self.cur_line < self.top_line {
            self.top_line = self.cur_line;
        } else if self.cur_line >= self.top_line + rows {
            self.top_line = self.cur_line + 1 - rows;
        }
        let cols = self.text_cols.max(1);
        let dcol = self.display_col(self.cur_line, self.cur_col);
        if dcol < self.left_col {
            self.left_col = dcol;
        } else if dcol >= self.left_col + cols {
            self.left_col = dcol + 1 - cols;
        }
    }

    pub fn cursor_display_col(&self) -> usize {
        self.display_col(self.cur_line, self.cur_col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn plain(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn type_str(ed: &mut Editor, s: &str) {
        for c in s.chars() {
            ed.handle_key(plain(c));
        }
    }

    fn text(ed: &Editor) -> String {
        buffer::lines_to_text(&ed.buffer.lines)
    }

    fn new_editor() -> Editor {
        Editor::new(Buffer::new())
    }

    #[test]
    fn insert_newline_backspace() {
        let mut ed = new_editor();
        type_str(&mut ed, "hello");
        assert_eq!(text(&ed), "hello");
        ed.handle_key(key(KeyCode::Enter));
        type_str(&mut ed, "world");
        assert_eq!(text(&ed), "hello\nworld");
        ed.handle_key(key(KeyCode::Backspace));
        assert_eq!(text(&ed), "hello\nworl");
        assert_eq!((ed.cur_line, ed.cur_col), (1, 4));
    }

    #[test]
    fn backspace_at_line_start_joins_lines() {
        let mut ed = new_editor();
        type_str(&mut ed, "abc");
        ed.handle_key(key(KeyCode::Enter));
        type_str(&mut ed, "def");
        ed.cur_col = 0;
        ed.handle_key(key(KeyCode::Backspace));
        assert_eq!(text(&ed), "abcdef");
        assert_eq!((ed.cur_line, ed.cur_col), (0, 3));
    }

    #[test]
    fn overwrite_mode_replaces_chars() {
        let mut ed = new_editor();
        type_str(&mut ed, "hello");
        ed.cur_col = 1;
        ed.handle_key(ctrl('v')); // toggle to overwrite
        assert!(!ed.insert_mode);
        type_str(&mut ed, "XY");
        assert_eq!(text(&ed), "hXYlo");
    }

    #[test]
    fn word_movement() {
        let mut ed = new_editor();
        type_str(&mut ed, "foo bar baz");
        ed.cur_col = 0;
        ed.word_right();
        assert_eq!((ed.cur_line, ed.cur_col), (0, 4));
        ed.word_right();
        assert_eq!((ed.cur_line, ed.cur_col), (0, 8));
        ed.word_left();
        assert_eq!((ed.cur_line, ed.cur_col), (0, 4));
        ed.word_left();
        assert_eq!((ed.cur_line, ed.cur_col), (0, 0));
    }

    #[test]
    fn delete_word_right_removes_word_and_trailing_space() {
        let mut ed = new_editor();
        type_str(&mut ed, "foo bar baz");
        ed.cur_col = 0;
        ed.delete_word_right();
        assert_eq!(text(&ed), "bar baz");
        assert_eq!((ed.cur_line, ed.cur_col), (0, 0));
    }

    #[test]
    fn delete_line_via_command() {
        let mut ed = new_editor();
        type_str(&mut ed, "one");
        ed.handle_key(key(KeyCode::Enter));
        type_str(&mut ed, "two");
        ed.handle_key(key(KeyCode::Enter));
        type_str(&mut ed, "three");
        ed.cur_line = 1;
        ed.handle_key(ctrl('y'));
        assert_eq!(text(&ed), "one\nthree");
    }

    #[test]
    fn block_copy_via_keys() {
        let mut ed = new_editor();
        type_str(&mut ed, "one two three");
        ed.cur_col = 4;
        ed.handle_key(ctrl('k'));
        ed.handle_key(plain('b'));
        ed.cur_col = 7;
        ed.handle_key(ctrl('k'));
        ed.handle_key(plain('k'));
        ed.cur_col = 13;
        ed.handle_key(ctrl('k'));
        ed.handle_key(plain('c'));
        assert_eq!(text(&ed), "one two threetwo");
    }

    #[test]
    fn block_move_via_keys() {
        let mut ed = new_editor();
        type_str(&mut ed, "one two three");
        ed.cur_col = 4;
        ed.handle_key(ctrl('k'));
        ed.handle_key(plain('b'));
        ed.cur_col = 8;
        ed.handle_key(ctrl('k'));
        ed.handle_key(plain('k'));
        ed.cur_col = 13;
        ed.handle_key(ctrl('k'));
        ed.handle_key(plain('v'));
        assert_eq!(text(&ed), "one threetwo ");
    }

    #[test]
    fn block_delete_via_keys() {
        let mut ed = new_editor();
        type_str(&mut ed, "one two three");
        ed.cur_col = 4;
        ed.handle_key(ctrl('k'));
        ed.handle_key(plain('b'));
        ed.cur_col = 8;
        ed.handle_key(ctrl('k'));
        ed.handle_key(plain('k'));
        ed.handle_key(ctrl('k'));
        ed.handle_key(plain('y'));
        assert_eq!(text(&ed), "one three");
        assert_eq!((ed.cur_line, ed.cur_col), (0, 4));
    }

    #[test]
    fn find_moves_cursor_to_match() {
        let mut ed = new_editor();
        type_str(&mut ed, "the quick brown fox");
        ed.cur_col = 0;
        ed.handle_key(ctrl('q'));
        ed.handle_key(plain('f'));
        type_str(&mut ed, "brown");
        ed.handle_key(key(KeyCode::Enter));
        assert_eq!((ed.cur_line, ed.cur_col), (0, 10));
    }

    #[test]
    fn find_not_found_reports_message() {
        let mut ed = new_editor();
        type_str(&mut ed, "hello world");
        ed.cur_col = 0;
        ed.handle_key(ctrl('q'));
        ed.handle_key(plain('f'));
        type_str(&mut ed, "xyz");
        ed.handle_key(key(KeyCode::Enter));
        assert!(ed.message.contains("not found"));
    }

    #[test]
    fn replace_yes_no_all_flow() {
        let mut ed = new_editor();
        type_str(&mut ed, "cat cat cat");
        ed.cur_col = 0;
        ed.handle_key(ctrl('q'));
        ed.handle_key(plain('a'));
        type_str(&mut ed, "cat");
        ed.handle_key(key(KeyCode::Enter));
        type_str(&mut ed, "dog");
        ed.handle_key(key(KeyCode::Enter));
        ed.handle_key(plain('y'));
        ed.handle_key(plain('n'));
        ed.handle_key(plain('a'));
        assert_eq!(text(&ed), "dog cat dog");
    }

    #[test]
    fn save_without_filename_prompts_then_saves() {
        let dir = std::env::temp_dir().join(format!("mustang-editor-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("saved.txt");

        let mut ed = new_editor();
        type_str(&mut ed, "hello file");
        ed.handle_key(ctrl('k'));
        ed.handle_key(plain('s'));
        assert!(ed.prompt.is_some());
        type_str(&mut ed, path.to_str().unwrap());
        ed.handle_key(key(KeyCode::Enter));
        assert!(ed.prompt.is_none());
        assert!(!ed.buffer.dirty);

        let saved = std::fs::read_to_string(&path).unwrap();
        assert_eq!(saved, "hello file");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn quit_without_changes_is_immediate() {
        let mut ed = new_editor();
        ed.handle_key(ctrl('k'));
        ed.handle_key(plain('q'));
        assert!(ed.quit);
    }

    #[test]
    fn quit_with_unsaved_changes_prompts_for_confirmation() {
        let mut ed = new_editor();
        type_str(&mut ed, "unsaved");
        ed.handle_key(ctrl('k'));
        ed.handle_key(plain('q'));
        assert!(!ed.quit);
        assert!(ed.prompt.is_some());
        ed.handle_key(plain('y'));
        ed.handle_key(key(KeyCode::Enter));
        assert!(ed.quit);
    }
}
