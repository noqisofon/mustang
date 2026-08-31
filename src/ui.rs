use std::io::{self, Write};

use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{cursor, QueueableCommand};

use crate::editor::Editor;
use crate::keymap::PrefixState;
use crate::util;

pub fn draw<W: Write>(out: &mut W, editor: &mut Editor, term_cols: u16, term_rows: u16) -> io::Result<()> {
    let cols = (term_cols as usize).max(1);
    let rows = (term_rows as usize).max(1);

    let bottom_reserved = 1 + if editor.menu_visible { 3 } else { 0 };
    let text_rows = rows.saturating_sub(1 + bottom_reserved).max(1);
    editor.set_viewport(text_rows, cols);
    editor.ensure_visible();

    out.queue(cursor::Hide)?;
    out.queue(Clear(ClearType::All))?;

    draw_ruler(out, editor, cols)?;
    draw_text(out, editor, text_rows, cols)?;

    let menu_row = (1 + text_rows).min(rows.saturating_sub(1));
    if editor.menu_visible {
        draw_menu(out, editor, menu_row as u16, cols)?;
    }
    let status_row = (menu_row + if editor.menu_visible { 3 } else { 0 }).min(rows.saturating_sub(1));
    draw_status(out, editor, status_row as u16, cols)?;

    if editor.help_visible {
        draw_help_overlay(out, cols, rows)?;
    }

    let screen_row = (1 + editor.cur_line.saturating_sub(editor.top_line)).min(rows.saturating_sub(1));
    let screen_col = editor
        .cursor_display_col()
        .saturating_sub(editor.left_col)
        .min(cols.saturating_sub(1));
    out.queue(cursor::MoveTo(screen_col as u16, screen_row as u16))?;
    out.queue(cursor::Show)?;
    out.flush()
}

fn draw_ruler<W: Write>(out: &mut W, editor: &Editor, cols: usize) -> io::Result<()> {
    out.queue(cursor::MoveTo(0, 0))?;
    out.queue(SetAttribute(Attribute::Reverse))?;
    let mut ruler = String::with_capacity(cols);
    for i in 0..cols {
        let col_num = editor.left_col + i + 1;
        if i == 0 {
            ruler.push('L');
        } else if col_num.is_multiple_of(10) {
            ruler.push_str(&((col_num / 10) % 10).to_string());
        } else {
            ruler.push('.');
        }
    }
    out.queue(Print(ruler))?;
    out.queue(SetAttribute(Attribute::Reset))?;
    Ok(())
}

fn draw_text<W: Write>(out: &mut W, editor: &Editor, text_rows: usize, cols: usize) -> io::Result<()> {
    let marks = if editor.block_visible {
        editor.normalized_marks()
    } else {
        None
    };
    for vi in 0..text_rows {
        out.queue(cursor::MoveTo(0, (vi + 1) as u16))?;
        let line_idx = editor.top_line + vi;
        if line_idx >= editor.buffer.num_lines() {
            out.queue(SetForegroundColor(Color::DarkBlue))?;
            out.queue(Print("~"))?;
            out.queue(ResetColor)?;
            continue;
        }
        let chars = &editor.buffer.lines[line_idx];
        let mut col = 0usize;
        let mut printed = 0usize;
        let mut in_highlight = false;
        for (ci, &ch) in chars.iter().enumerate() {
            if printed >= cols {
                break;
            }
            let w = util::char_display_width(ch, col, util::TAB_SIZE);
            let visible = col + w > editor.left_col && col >= editor.left_col;
            if visible {
                let is_marked = marks.is_some_and(|(s, e)| {
                    let pos = (line_idx, ci);
                    pos >= s && pos < e
                });
                if is_marked && !in_highlight {
                    out.queue(SetAttribute(Attribute::Reverse))?;
                    in_highlight = true;
                } else if !is_marked && in_highlight {
                    out.queue(SetAttribute(Attribute::Reset))?;
                    in_highlight = false;
                }
                if ch == '\t' {
                    let n = w.min(cols - printed);
                    out.queue(Print(" ".repeat(n)))?;
                    printed += n;
                } else {
                    out.queue(Print(ch))?;
                    printed += w;
                }
            }
            col += w;
        }
        if in_highlight {
            out.queue(SetAttribute(Attribute::Reset))?;
        }
    }
    Ok(())
}

fn draw_menu<W: Write>(out: &mut W, editor: &Editor, row_start: u16, cols: usize) -> io::Result<()> {
    let lines: [&str; 3] = match editor.prefix {
        PrefixState::K => [
            "^K B=mark begin  K=mark end  C=copy  V=move  Y=delete  H=hide/show block",
            "^K W=write block to file  R=read file  S/D=save  X=save & exit",
            "^K Q=quit without saving                              Esc=cancel",
        ],
        PrefixState::Q => [
            "^Q S=line start  D=line end  E=screen top  X=screen bottom",
            "^Q R=doc start   C=doc end   F=find   A=find&replace   Y=del to EOL",
            "^Q B=goto block begin  K=goto block end  L=goto line   Esc=cancel",
        ],
        PrefixState::O => ["^O H=hide/show this key menu", "", "                 Esc=cancel"],
        PrefixState::None => [
            "^E up  ^X down  ^S left  ^D right   ^A word-left  ^F word-right  ^R/^C page up/down",
            "^G del-char  ^H bksp  ^T del-word  ^Y del-line  ^N open-line  ^V ins/ovr  Tab",
            "^K block/file menu   ^Q quick-move/find menu   ^L repeat find   ^J/F1 help",
        ],
    };
    for (i, text) in lines.iter().enumerate() {
        out.queue(cursor::MoveTo(0, row_start + i as u16))?;
        out.queue(SetForegroundColor(Color::Green))?;
        let mut s = text.to_string();
        if s.chars().count() > cols {
            s = s.chars().take(cols).collect();
        }
        out.queue(Print(s))?;
        out.queue(ResetColor)?;
    }
    Ok(())
}

fn draw_status<W: Write>(out: &mut W, editor: &Editor, row: u16, cols: usize) -> io::Result<()> {
    out.queue(cursor::MoveTo(0, row))?;
    out.queue(SetAttribute(Attribute::Reverse))?;
    let content = if editor.prompt.is_some() {
        format!("{} {}", editor.prompt_label, editor.prompt_input)
    } else {
        let mode = if editor.insert_mode { "INS" } else { "OVR" };
        let dirty = if editor.buffer.dirty { "*" } else { " " };
        let block = if editor.mark_start.is_some() || editor.mark_end.is_some() {
            if editor.block_visible {
                " [BLOCK MARKED]"
            } else {
                " [block hidden]"
            }
        } else {
            ""
        };
        format!(
            "{}{}  L{}:C{}  {}  {}{}",
            editor.buffer.display_name(),
            dirty,
            editor.cur_line + 1,
            editor.cur_col + 1,
            mode,
            editor.message,
            block
        )
    };
    let mut s = content;
    let len = s.chars().count();
    if len > cols {
        s = s.chars().take(cols).collect();
    } else {
        s.push_str(&" ".repeat(cols - len));
    }
    out.queue(Print(s))?;
    out.queue(SetAttribute(Attribute::Reset))?;
    Ok(())
}

fn draw_help_overlay<W: Write>(out: &mut W, cols: usize, rows: usize) -> io::Result<()> {
    let help_lines = [
        "mustang -- WordStar 3.3 style editor",
        "",
        "Cursor:  ^E up        ^X down        ^S left        ^D right",
        "         ^A word-left            ^F word-right",
        "         ^Q S line start          ^Q D line end",
        "         ^Q E screen top          ^Q X screen bottom",
        "         ^Q R doc start           ^Q C doc end",
        "         ^R/PgUp page up          ^C/PgDn page down",
        "         ^Q L go to line",
        "",
        "Editing: ^G del char right (Del)  ^H del char left (Bksp)",
        "         ^T del word right        ^Y del line",
        "         ^Q Y del to end of line   ^N open line",
        "         ^V toggle insert/overwrite mode",
        "",
        "Block:   ^K B mark begin   ^K K mark end",
        "         ^K C copy block   ^K V move block   ^K Y delete block",
        "         ^K H hide/show    ^K W write to file   ^K R read file in",
        "         ^Q B goto block begin     ^Q K goto block end",
        "",
        "Search:  ^Q F find    ^Q A find & replace    ^L repeat find",
        "",
        "File:    ^K S/D save    ^K X save & exit    ^K Q quit without saving",
        "",
        "Misc:    ^O H hide/show key menu    ^J or F1 this help",
        "",
        "              -- press any key to continue --",
    ];
    let box_h = help_lines.len().min(rows);
    let box_w = (help_lines.iter().map(|s| s.chars().count()).max().unwrap_or(0) + 4).min(cols);
    let start_row = rows.saturating_sub(box_h) / 2;
    let start_col = cols.saturating_sub(box_w) / 2;
    for (i, line) in help_lines.iter().take(box_h).enumerate() {
        out.queue(cursor::MoveTo(start_col as u16, (start_row + i) as u16))?;
        out.queue(SetAttribute(Attribute::Reverse))?;
        let mut s = format!("  {}", line);
        let len = s.chars().count();
        if len > box_w {
            s = s.chars().take(box_w).collect();
        } else {
            s.push_str(&" ".repeat(box_w - len));
        }
        out.queue(Print(s))?;
        out.queue(SetAttribute(Attribute::Reset))?;
    }
    Ok(())
}
