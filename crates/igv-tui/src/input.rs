use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::app::action::Action;

#[derive(Debug, Default)]
pub struct InputState {
    /// True when a leading bookmark prefix has been observed
    /// (`m` for set, `'` for jump).
    pub pending_bookmark: Option<BookmarkOp>,
}

#[derive(Debug, Clone, Copy)]
pub enum BookmarkOp {
    Set,
    Jump,
}

impl InputState {
    pub fn map(
        &mut self,
        event: &Event,
        command_open: bool,
    ) -> Action {
        if let Event::Key(KeyEvent { code, modifiers, .. }) = event {
            // While the command palette is open, only Esc/Enter/typing matter.
            if command_open {
                return match code {
                    KeyCode::Esc => Action::CommandCancel,
                    _ => Action::None, // command.rs handles typing
                };
            }
            // Ctrl-C exits.
            if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c')) {
                return Action::Quit;
            }

            // Bookmark prefix handling
            if let Some(op) = self.pending_bookmark.take() {
                if let KeyCode::Char(c) = code {
                    return match op {
                        BookmarkOp::Set => Action::SetBookmark(*c),
                        BookmarkOp::Jump => Action::JumpBookmark(*c),
                    };
                }
                return Action::None;
            }

            return match code {
                KeyCode::Char('q') => Action::Quit,
                KeyCode::Char('a') | KeyCode::Left => Action::MoveBackward,
                KeyCode::Char('d') | KeyCode::Right => Action::MoveForward,
                KeyCode::Char('w') | KeyCode::Up => Action::Zoom { zoom_in: true },
                KeyCode::Char('s') | KeyCode::Down => Action::Zoom { zoom_in: false },
                KeyCode::Char('t') => Action::ToggleTheme,
                KeyCode::Char(':') | KeyCode::Char('g') => Action::OpenCommand,
                KeyCode::Char('m') => {
                    self.pending_bookmark = Some(BookmarkOp::Set);
                    Action::None
                }
                KeyCode::Char('\'') => {
                    self.pending_bookmark = Some(BookmarkOp::Jump);
                    Action::None
                }
                _ => Action::None,
            };
        }
        Action::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(c: char) -> Event {
        Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn d_moves_forward() {
        let mut s = InputState::default();
        assert!(matches!(s.map(&key('d'), false), Action::MoveForward));
    }

    #[test]
    fn m_then_a_sets_bookmark_a() {
        let mut s = InputState::default();
        assert!(matches!(s.map(&key('m'), false), Action::None));
        assert!(matches!(s.map(&key('a'), false), Action::SetBookmark('a')));
    }

    #[test]
    fn quote_then_a_jumps_bookmark_a() {
        let mut s = InputState::default();
        assert!(matches!(s.map(&key('\''), false), Action::None));
        assert!(matches!(s.map(&key('a'), false), Action::JumpBookmark('a')));
    }
}
