use std::io::ErrorKind;

use crossterm::event::{Event, KeyEventKind};
use pax_core::PaxError;

use crate::action::Action;
use crate::job::{JobKind, JobOutcome};
use crate::message::{Effect, Message};
use crate::status::StatusBar;
use crate::ui::library::{LibraryScreen, LibraryState};

/// Which top-level view is showing. Gains a variant per screen as each one
/// is built (see docs/status.md's build order).
pub enum Screen {
    Library,
}

/// Tracks a `g` press waiting for a second key (`gg` -> go to top). Reset on
/// any key that isn't the completing `g`.
#[derive(Default)]
pub struct PendingInput {
    pub g_pressed: bool,
}

pub struct App {
    pub screen: Screen,
    pub should_quit: bool,
    pub pending: PendingInput,
    pub status: StatusBar,
    pub library: LibraryScreen,
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Library,
            should_quit: false,
            pending: PendingInput::default(),
            status: StatusBar::default(),
            library: LibraryScreen::default(),
        }
    }

    /// Effects to run once, before the event loop starts.
    pub fn init_effects(&self) -> Vec<Effect> {
        vec![Effect::Spawn(JobKind::LoadLibrary)]
    }

    /// The reducer: mutates `self` in response to a `Message`, returning any
    /// `Effect`s the main loop should perform. No I/O happens here directly.
    pub fn update(&mut self, msg: Message) -> Vec<Effect> {
        match msg {
            Message::Tick => Vec::new(),
            Message::Input(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                match crate::keymap::map_key(&mut self.pending, key) {
                    Some(action) => self.apply(action),
                    None => Vec::new(),
                }
            }
            Message::Input(_) => Vec::new(),
            Message::Job(_id, outcome) => self.apply_job_outcome(outcome),
        }
    }

    fn apply(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::Quit => {
                self.should_quit = true;
                vec![Effect::Quit]
            }
            Action::MoveDown => {
                self.library.move_down();
                Vec::new()
            }
            Action::MoveUp => {
                self.library.move_up();
                Vec::new()
            }
            Action::GoTop => {
                self.library.go_top();
                Vec::new()
            }
            Action::GoBottom => {
                self.library.go_bottom();
                Vec::new()
            }
        }
    }

    fn apply_job_outcome(&mut self, outcome: JobOutcome) -> Vec<Effect> {
        match outcome {
            JobOutcome::Library(Ok(papers)) => {
                self.library.state = LibraryState::Loaded(papers);
                self.library.selected = 0;
            }
            JobOutcome::Library(Err(PaxError::Io(e))) if e.kind() == ErrorKind::NotFound => {
                self.library.state = LibraryState::NotInitialized;
            }
            JobOutcome::Library(Err(e)) => {
                self.status.error(e.to_string());
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_library_ok_populates_state() {
        let mut app = App::new();
        app.update(Message::Job(1, JobOutcome::Library(Ok(Vec::new()))));
        assert!(matches!(app.library.state, LibraryState::Loaded(ref p) if p.is_empty()));
    }

    #[test]
    fn missing_library_file_is_not_initialized_not_an_error() {
        let mut app = App::new();
        let io_err = std::io::Error::new(ErrorKind::NotFound, "no such file");
        app.update(Message::Job(1, JobOutcome::Library(Err(PaxError::Io(io_err)))));
        assert!(matches!(app.library.state, LibraryState::NotInitialized));
        assert!(app.status.message.is_none());
    }

    #[test]
    fn other_library_errors_go_to_the_status_bar() {
        let mut app = App::new();
        app.update(Message::Job(
            1,
            JobOutcome::Library(Err(PaxError::NoSuchPaper("x".to_string()))),
        ));
        assert!(app.status.message.is_some());
        assert!(matches!(app.library.state, LibraryState::Loading));
    }

    #[test]
    fn quit_action_sets_should_quit_and_returns_quit_effect() {
        let mut app = App::new();
        let effects = app.apply(Action::Quit);
        assert!(app.should_quit);
        assert!(matches!(effects.as_slice(), [Effect::Quit]));
    }
}
