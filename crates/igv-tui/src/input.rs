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
        self.map_with_help(event, command_open, false)
    }

    pub fn map_with_help(
        &mut self,
        event: &Event,
        command_open: bool,
        help_open: bool,
    ) -> Action {
        if let Event::Key(KeyEvent { code, modifiers, .. }) = event {
            // While the help overlay is open, Ctrl-C still quits; any other
            // key dismisses the overlay (top-style any-key-to-close).
            if help_open {
                if modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(code, KeyCode::Char('c'))
                {
                    return Action::Quit;
                }
                return Action::CloseHelp;
            }
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
                KeyCode::Char('a') | KeyCode::Left => {
                    Action::Move { forward: false, large: true }
                }
                KeyCode::Char('d') | KeyCode::Right => {
                    Action::Move { forward: true, large: true }
                }
                KeyCode::Char('h') => Action::Move { forward: false, large: false },
                KeyCode::Char('l') => Action::Move { forward: true, large: false },
                KeyCode::Char('w') | KeyCode::Up => Action::Zoom { zoom_in: true },
                KeyCode::Char('s') | KeyCode::Down => Action::Zoom { zoom_in: false },
                KeyCode::Char('j') => Action::ScrollAlignments(1),
                KeyCode::Char('k') => Action::ScrollAlignments(-1),
                KeyCode::Char('+') | KeyCode::Char('=') => Action::ResizeAlignments(1),
                KeyCode::Char('-') | KeyCode::Char('_') => Action::ResizeAlignments(-1),
                KeyCode::Char(']') => Action::ResizeCoverage(1),
                KeyCode::Char('[') => Action::ResizeCoverage(-1),
                KeyCode::Char('\\') => Action::ToggleSignalSharedScale,
                KeyCode::Char('}') => Action::ResizeSignal(1),
                KeyCode::Char('{') => Action::ResizeSignal(-1),
                KeyCode::Char('t') => Action::ToggleTheme,
                KeyCode::Char(':') | KeyCode::Char('g') => Action::OpenCommand,
                KeyCode::Char('?') => Action::ToggleHelp,
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
    fn d_moves_forward_full_window() {
        let mut s = InputState::default();
        assert!(matches!(
            s.map(&key('d'), false),
            Action::Move { forward: true, large: true }
        ));
    }

    #[test]
    fn l_moves_forward_fine() {
        let mut s = InputState::default();
        assert!(matches!(
            s.map(&key('l'), false),
            Action::Move { forward: true, large: false }
        ));
    }

    #[test]
    fn h_moves_backward_fine() {
        let mut s = InputState::default();
        assert!(matches!(
            s.map(&key('h'), false),
            Action::Move { forward: false, large: false }
        ));
    }


    #[test]
    fn j_scrolls_alignments_down() {
        let mut s = InputState::default();
        assert!(matches!(s.map(&key('j'), false), Action::ScrollAlignments(1)));
    }

    #[test]
    fn plus_grows_alignments() {
        let mut s = InputState::default();
        assert!(matches!(s.map(&key('+'), false), Action::ResizeAlignments(1)));
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

    #[test]
    fn backslash_toggles_signal_shared_scale() {
        let mut s = InputState::default();
        assert!(matches!(
            s.map(&key('\\'), false),
            Action::ToggleSignalSharedScale
        ));
    }

    #[test]
    fn close_brace_grows_signal_track() {
        let mut s = InputState::default();
        assert!(matches!(s.map(&key('}'), false), Action::ResizeSignal(1)));
    }

    #[test]
    fn open_brace_shrinks_signal_track() {
        let mut s = InputState::default();
        assert!(matches!(s.map(&key('{'), false), Action::ResizeSignal(-1)));
    }

    #[test]
    fn question_mark_toggles_help() {
        let mut s = InputState::default();
        assert!(matches!(s.map(&key('?'), false), Action::ToggleHelp));
    }

    #[test]
    fn any_key_closes_help_when_open() {
        let mut s = InputState::default();
        // While the overlay is open, an arbitrary key dismisses it.
        assert!(matches!(
            s.map_with_help(&key('a'), false, true),
            Action::CloseHelp
        ));
        assert!(matches!(
            s.map_with_help(&key('?'), false, true),
            Action::CloseHelp
        ));
    }

    #[test]
    fn ctrl_c_still_quits_when_help_open() {
        let mut s = InputState::default();
        let ctrl_c = Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        assert!(matches!(
            s.map_with_help(&ctrl_c, false, true),
            Action::Quit
        ));
    }
}
