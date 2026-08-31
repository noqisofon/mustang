mod buffer;
mod editor;
mod keymap;
mod ui;
mod util;

use std::env;
use std::io::{self, Write};
use std::panic;
use std::path::PathBuf;

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};

use buffer::Buffer;
use editor::Editor;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let buffer = match args.first() {
        Some(path) => {
            let p = PathBuf::from(path);
            if p.exists() {
                match Buffer::load(&p) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("mustang: failed to read {}: {}", p.display(), e);
                        std::process::exit(1);
                    }
                }
            } else {
                let mut b = Buffer::new();
                b.filename = Some(p);
                b
            }
        }
        None => Buffer::new(),
    };

    let mut editor = Editor::new(buffer);

    setup_panic_hook();
    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;

    let result = run(&mut stdout, &mut editor);

    let _ = execute!(stdout, cursor::Show, LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();

    result
}

/// Make sure a panic doesn't leave the user's terminal stuck in raw mode /
/// the alternate screen with an invisible cursor.
fn setup_panic_hook() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let mut stdout = io::stdout();
        let _ = execute!(stdout, cursor::Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
        let _ = stdout.flush();
        default_hook(info);
    }));
}

fn run<W: Write>(stdout: &mut W, editor: &mut Editor) -> io::Result<()> {
    let (mut cols, mut rows) = terminal::size()?;
    ui::draw(stdout, editor, cols, rows)?;

    while !editor.quit {
        let ev = event::read()?;
        match ev {
            // Windows Terminal (via the Win32 console API) delivers both a
            // key-down and a key-up event per keystroke; only act on Press
            // so every key isn't handled twice.
            Event::Key(k) if k.kind == KeyEventKind::Press => editor.handle_key(k),
            Event::Resize(c, r) => {
                cols = c;
                rows = r;
            }
            _ => continue,
        }
        ui::draw(stdout, editor, cols, rows)?;
    }
    Ok(())
}
