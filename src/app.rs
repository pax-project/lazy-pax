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

/// Normal (navigation) vs. text-entry input. `Insert` carries which field
/// is being edited, so a generic `InputChar`/`InputBackspace`/`SubmitInput`
/// path can route to the right buffer.
pub enum Mode {
    Normal,
    Insert(InsertTarget),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertTarget {
    LibraryFilter,
}

/// Tracks a `g` press waiting for a second key (`gg` -> go to top). Reset on
/// any key that isn't the completing `g`.
#[derive(Default)]
pub struct PendingInput {
    pub g_pressed: bool,
}

pub struct App {
    pub screen: Screen,
    pub mode: Mode,
    pub should_quit: bool,
    pub pending: PendingInput,
    pub status: StatusBar,
    pub library: LibraryScreen,
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Library,
            mode: Mode::Normal,
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
                match crate::keymap::map_key(&self.mode, &mut self.pending, key) {
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
            Action::EnterInsert(target) => {
                match target {
                    InsertTarget::LibraryFilter => self.library.begin_filter_edit(),
                }
                self.mode = Mode::Insert(target);
                Vec::new()
            }
            Action::CancelInput => {
                self.mode = Mode::Normal;
                Vec::new()
            }
            Action::SubmitInput => {
                if let Mode::Insert(target) = self.mode {
                    match target {
                        InsertTarget::LibraryFilter => self.library.submit_filter_edit(),
                    }
                }
                self.mode = Mode::Normal;
                Vec::new()
            }
            Action::InputChar(c) => {
                if let Mode::Insert(InsertTarget::LibraryFilter) = self.mode {
                    self.library.filter_buffer.push(c);
                }
                Vec::new()
            }
            Action::InputBackspace => {
                if let Mode::Insert(InsertTarget::LibraryFilter) = self.mode {
                    self.library.filter_buffer.pop();
                }
                Vec::new()
            }
            Action::ClearFilter => {
                self.library.clear_filter();
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

    fn with_two_papers() -> App {
        use pax_core::{Artifact, Identity, Local, Paper};
        let mut app = App::new();
        let papers = vec![
            Paper {
                identity: Identity {
                    title: "On Computable Numbers".to_string(),
                    authors: vec!["Alan Turing".to_string()],
                    year: Some(1936),
                    ..Default::default()
                },
                artifact: Artifact::default(),
                local: Local {
                    citation_key: "turing1936".to_string(),
                    ..Default::default()
                },
            },
            Paper {
                identity: Identity {
                    title: "A Universal Modular Actor Formalism".to_string(),
                    authors: vec!["Carl Hewitt".to_string()],
                    year: Some(1973),
                    ..Default::default()
                },
                artifact: Artifact::default(),
                local: Local {
                    citation_key: "hewitt1973".to_string(),
                    ..Default::default()
                },
            },
        ];
        app.update(Message::Job(1, JobOutcome::Library(Ok(papers))));
        app
    }

    #[test]
    fn slash_enter_type_enter_applies_a_filter() {
        let mut app = with_two_papers();
        app.apply(Action::EnterInsert(InsertTarget::LibraryFilter));
        assert!(matches!(app.mode, Mode::Insert(InsertTarget::LibraryFilter)));
        for c in "hewitt".chars() {
            app.apply(Action::InputChar(c));
        }
        app.apply(Action::SubmitInput);
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(app.library.query, "hewitt");
        assert_eq!(app.library.visible_papers().unwrap().len(), 1);
    }

    #[test]
    fn cancel_input_discards_the_buffer_without_applying_it() {
        let mut app = with_two_papers();
        app.apply(Action::EnterInsert(InsertTarget::LibraryFilter));
        for c in "hewitt".chars() {
            app.apply(Action::InputChar(c));
        }
        app.apply(Action::CancelInput);
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(app.library.query, "");
        assert_eq!(app.library.visible_papers().unwrap().len(), 2);
    }

    #[test]
    fn esc_in_normal_mode_clears_an_applied_filter() {
        let mut app = with_two_papers();
        app.library.query = "hewitt".to_string();
        app.apply(Action::ClearFilter);
        assert_eq!(app.library.query, "");
        assert_eq!(app.library.visible_papers().unwrap().len(), 2);
    }

    #[test]
    fn reopening_the_filter_prefills_the_buffer_with_the_applied_query() {
        let mut app = with_two_papers();
        app.library.query = "turing".to_string();
        app.apply(Action::EnterInsert(InsertTarget::LibraryFilter));
        assert_eq!(app.library.filter_buffer, "turing");
    }
}
