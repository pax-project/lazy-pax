use crossterm::event::{Event, KeyEventKind};

use crate::action::Action;
use crate::message::{Effect, Message};

/// Which top-level view is showing. Gains a variant per screen as each one
/// is built (see docs/status.md's build order).
pub enum Screen {
    Library,
}

pub struct App {
    pub screen: Screen,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Library,
            should_quit: false,
        }
    }

    /// The reducer: mutates `self` in response to a `Message`, returning any
    /// `Effect`s the main loop should perform. No I/O happens here directly.
    pub fn update(&mut self, msg: Message) -> Vec<Effect> {
        match msg {
            Message::Tick => Vec::new(),
            Message::Input(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                match crate::keymap::map_key(key) {
                    Some(action) => self.apply(action),
                    None => Vec::new(),
                }
            }
            Message::Input(_) => Vec::new(),
        }
    }

    fn apply(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::Quit => {
                self.should_quit = true;
                vec![Effect::Quit]
            }
        }
    }
}
