use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};

use crate::app::action::Action;

#[derive(Debug, Default)]
pub struct InputState {
    pub pending_bookmark: Option<BookmarkOp>,
    pub drag_start: Option<(u16, u16)>,
}

#[derive(Debug, Clone, Copy)]
pub enum BookmarkOp {
    Set,
    Jump,
}

impl InputState {
    pub fn map(&mut self, event: &Event, command_open: bool) -> Action {
        self.map_full(event, command_open, false, false)
    }

    pub fn map_with_help(&mut self, event: &Event, command_open: bool, help_open: bool) -> Action {
        self.map_full(event, command_open, help_open, false)
    }

    pub fn map_full(
        &mut self,
        event: &Event,
        command_open: bool,
        help_open: bool,
        inspector_open: bool,
    ) -> Action {
        match event {
            Event::Mouse(me) => {
                if command_open || help_open {
                    return Action::None;
                }
                if inspector_open {
                    if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
                        return Action::CloseInspector;
                    }
                    return Action::None;
                }
                match me.kind {
                    MouseEventKind::ScrollDown => Action::ScrollAlignments(3),
                    MouseEventKind::ScrollUp => Action::ScrollAlignments(-3),
                    MouseEventKind::Down(MouseButton::Left) => {
                        self.drag_start = Some((me.column, me.row));
                        Action::None
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        if let Some((sx, sy)) = self.drag_start.take() {
                            if sx.abs_diff(me.column) >= 2 && sy == me.row {
                                Action::MouseDrag {
                                    start_x: sx.min(me.column),
                                    end_x: sx.max(me.column),
                                    y: sy,
                                }
                            } else {
                                Action::MouseClick {
                                    x: me.column,
                                    y: me.row,
                                }
                            }
                        } else {
                            Action::MouseClick {
                                x: me.column,
                                y: me.row,
                            }
                        }
                    }
                    _ => Action::None,
                }
            }
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => {
                if inspector_open {
                    if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c'))
                    {
                        return Action::Quit;
                    }
                    if matches!(code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                        return Action::CloseInspector;
                    }
                    return Action::None;
                }
                if help_open {
                    if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c'))
                    {
                        return Action::Quit;
                    }
                    return Action::CloseHelp;
                }
                if command_open {
                    return match code {
                        KeyCode::Esc => Action::CommandCancel,
                        _ => Action::None,
                    };
                }
                if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c')) {
                    return Action::Quit;
                }
                if let Some(op) = self.pending_bookmark.take() {
                    if let KeyCode::Char(c) = code {
                        return match op {
                            BookmarkOp::Set => Action::SetBookmark(*c),
                            BookmarkOp::Jump => Action::JumpBookmark(*c),
                        };
                    }
                    return Action::None;
                }

                match code {
                    KeyCode::Char('q') => Action::Quit,
                    KeyCode::Tab => Action::NextPane,
                    KeyCode::Char('a') | KeyCode::Left => Action::Move {
                        forward: false,
                        large: true,
                    },
                    KeyCode::Char('d') | KeyCode::Right => Action::Move {
                        forward: true,
                        large: true,
                    },
                    KeyCode::Char('h') => Action::Move {
                        forward: false,
                        large: false,
                    },
                    KeyCode::Char('l') => Action::Move {
                        forward: true,
                        large: false,
                    },
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
                    KeyCode::Char('>') => Action::ResizeLink(1),
                    KeyCode::Char('<') => Action::ResizeLink(-1),
                    KeyCode::Char('B') => Action::OpenBrowser,
                    KeyCode::Char('J') => Action::ToggleSashimi,
                    KeyCode::Char('P') => Action::TogglePairs,
                    KeyCode::Char('C') => Action::CycleColorMode,
                    KeyCode::Char('T') => Action::ToggleTranslation,
                    KeyCode::Char('G') => Action::ToggleGraphics,
                    KeyCode::Char('S') => Action::SaveSnapshot {
                        path: None,
                        format: crate::app::action::SnapshotFormat::Svg,
                    },
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
                }
            }
            _ => Action::None,
        }
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
            Action::Move {
                forward: true,
                large: true
            }
        ));
    }

    #[test]
    fn l_moves_forward_fine() {
        let mut s = InputState::default();
        assert!(matches!(
            s.map(&key('l'), false),
            Action::Move {
                forward: true,
                large: false
            }
        ));
    }
}
