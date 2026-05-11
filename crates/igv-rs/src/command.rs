use crossterm::event::{Event, KeyCode, KeyEvent};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::app::action::Action;

#[derive(Debug, Default)]
pub struct CommandPalette {
    pub input: Input,
    pub open: bool,
}

impl CommandPalette {
    pub fn open(&mut self) {
        self.open = true;
        self.input = Input::default();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.input = Input::default();
    }

    /// Returns an `Action::CommandSubmit` on Enter, `CommandCancel` on Esc,
    /// or `None` for typing.
    pub fn handle(&mut self, event: &Event) -> Action {
        if let Event::Key(KeyEvent { code, .. }) = event {
            match code {
                KeyCode::Enter => {
                    let buf = self.input.value().to_string();
                    self.close();
                    return Action::CommandSubmit(buf);
                }
                KeyCode::Esc => {
                    self.close();
                    return Action::CommandCancel;
                }
                _ => {
                    self.input.handle_event(event);
                    return Action::None;
                }
            }
        }
        Action::None
    }
}
