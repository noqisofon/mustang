use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Commands the editor can execute. Named after what they do rather than
/// the key that triggers them, since several keys can map to the same
/// action (e.g. Backspace and ^H).
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    ScreenTop,
    ScreenBottom,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
    GoToBlockBegin,
    GoToBlockEnd,
    GoToLinePrompt,

    InsertChar(char),
    Enter,
    Tab,
    OpenLine,
    DeleteCharRight,
    DeleteCharLeft,
    DeleteWordRight,
    DeleteLine,
    DeleteToEol,
    ToggleInsertMode,

    MarkBegin,
    MarkEnd,
    CopyBlock,
    MoveBlock,
    DeleteBlock,
    HideBlock,
    WriteBlockPrompt,
    ReadFilePrompt,

    Save,
    SaveDone,
    SaveExitPrompt,
    QuitPrompt,

    FindPrompt,
    ReplacePrompt,
    RepeatFind,

    ToggleMenu,
    Help,

    CancelPrefix,
    Beep,
    None,
}

/// Tracks whether we're mid-way through a WordStar two-key ^K / ^Q / ^O
/// prefix sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrefixState {
    #[default]
    None,
    K,
    Q,
    O,
}


/// Translate one key event into a Command, given (and possibly updating)
/// the current prefix state. Call only for KeyEventKind::Press events —
/// Windows Terminal (and other native-console backends) also delivers
/// Release events that must be filtered out by the caller first.
pub fn translate(ev: KeyEvent, prefix: &mut PrefixState) -> Command {
    let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);

    match *prefix {
        PrefixState::K => {
            *prefix = PrefixState::None;
            return match ev.code {
                KeyCode::Esc => Command::CancelPrefix,
                KeyCode::Char(c) => match c.to_ascii_uppercase() {
                    'B' => Command::MarkBegin,
                    'K' => Command::MarkEnd,
                    'C' => Command::CopyBlock,
                    'V' => Command::MoveBlock,
                    'Y' => Command::DeleteBlock,
                    'H' => Command::HideBlock,
                    'W' => Command::WriteBlockPrompt,
                    'R' => Command::ReadFilePrompt,
                    'S' => Command::Save,
                    'D' => Command::SaveDone,
                    'X' => Command::SaveExitPrompt,
                    'Q' => Command::QuitPrompt,
                    _ => Command::Beep,
                },
                _ => Command::Beep,
            };
        }
        PrefixState::Q => {
            *prefix = PrefixState::None;
            return match ev.code {
                KeyCode::Esc => Command::CancelPrefix,
                KeyCode::Char(c) => match c.to_ascii_uppercase() {
                    'S' => Command::LineStart,
                    'D' => Command::LineEnd,
                    'E' => Command::ScreenTop,
                    'X' => Command::ScreenBottom,
                    'R' => Command::DocStart,
                    'C' => Command::DocEnd,
                    'F' => Command::FindPrompt,
                    'A' => Command::ReplacePrompt,
                    'Y' => Command::DeleteToEol,
                    'B' => Command::GoToBlockBegin,
                    'K' => Command::GoToBlockEnd,
                    'L' => Command::GoToLinePrompt,
                    _ => Command::Beep,
                },
                _ => Command::Beep,
            };
        }
        PrefixState::O => {
            *prefix = PrefixState::None;
            return match ev.code {
                KeyCode::Esc => Command::CancelPrefix,
                KeyCode::Char(c) => match c.to_ascii_uppercase() {
                    'H' => Command::ToggleMenu,
                    _ => Command::Beep,
                },
                _ => Command::Beep,
            };
        }
        PrefixState::None => {}
    }

    if ctrl {
        if let KeyCode::Char(c) = ev.code {
            match c.to_ascii_lowercase() {
                'k' => {
                    *prefix = PrefixState::K;
                    return Command::None;
                }
                'q' => {
                    *prefix = PrefixState::Q;
                    return Command::None;
                }
                'o' => {
                    *prefix = PrefixState::O;
                    return Command::None;
                }
                'e' => return Command::MoveUp,
                'x' => return Command::MoveDown,
                's' => return Command::MoveLeft,
                'd' => return Command::MoveRight,
                'a' => return Command::WordLeft,
                'f' => return Command::WordRight,
                'r' => return Command::PageUp,
                'c' => return Command::PageDown,
                'g' => return Command::DeleteCharRight,
                'h' => return Command::DeleteCharLeft,
                't' => return Command::DeleteWordRight,
                'y' => return Command::DeleteLine,
                'n' => return Command::OpenLine,
                'v' => return Command::ToggleInsertMode,
                'l' => return Command::RepeatFind,
                'j' => return Command::Help,
                _ => return Command::Beep,
            }
        }
    }

    match ev.code {
        KeyCode::Up => Command::MoveUp,
        KeyCode::Down => Command::MoveDown,
        KeyCode::Left => Command::MoveLeft,
        KeyCode::Right => Command::MoveRight,
        KeyCode::Home => Command::LineStart,
        KeyCode::End => Command::LineEnd,
        KeyCode::PageUp => Command::PageUp,
        KeyCode::PageDown => Command::PageDown,
        KeyCode::Delete => Command::DeleteCharRight,
        KeyCode::Backspace => Command::DeleteCharLeft,
        KeyCode::Enter => Command::Enter,
        KeyCode::Tab => Command::Tab,
        KeyCode::F(1) => Command::Help,
        KeyCode::Esc => Command::None,
        KeyCode::Char(c) => Command::InsertChar(c),
        _ => Command::None,
    }
}
