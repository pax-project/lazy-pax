use std::io::ErrorKind;

use crossterm::event::{Event, KeyEventKind};
use pax_core::{FetchOutcome, PaxError};

use crate::action::Action;
use crate::job::{JobKind, JobOutcome};
use crate::message::{Effect, Message};
use crate::status::StatusBar;
use crate::ui::detail::{DetailScreen, DetailSubject};
use crate::ui::edit::EditScreen;
use crate::ui::library::{LibraryScreen, LibraryState};
use crate::ui::search::{SearchKind, SearchScreen, SearchState};

/// Which top-level view is showing. Gains a variant per screen as each one
/// is built (see docs/status.md's build order).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Library,
    Search,
    Detail,
    Edit,
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
    SearchQuery,
    EditTagAdd,
    EditNotes,
    EditRename,
    EditTitle,
    EditAuthors,
    EditYear,
    EditDoi,
}

/// Tracks a `g` press waiting for a second key (`gg` -> go to top). Reset on
/// any key that isn't the completing `g`.
#[derive(Default)]
pub struct PendingInput {
    pub g_pressed: bool,
}

pub struct App {
    pub screen: Screen,
    pub screen_stack: Vec<Screen>,
    pub mode: Mode,
    pub should_quit: bool,
    pub pending: PendingInput,
    pub status: StatusBar,
    /// Whether a background job is currently in flight. MVP keeps this to
    /// one job at a time rather than a queue — a job-triggering action
    /// pressed while this is true is rejected with a status message instead
    /// of racing a second job against the first (e.g. double-pressing `a`
    /// before `add_candidate` returns could otherwise declare the same
    /// paper twice, under two different citation keys).
    pub job_running: bool,
    pub library: LibraryScreen,
    pub search: SearchScreen,
    pub detail: DetailScreen,
    pub edit: EditScreen,
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Library,
            screen_stack: Vec::new(),
            mode: Mode::Normal,
            should_quit: false,
            pending: PendingInput::default(),
            status: StatusBar::default(),
            job_running: false,
            library: LibraryScreen::default(),
            search: SearchScreen::default(),
            detail: DetailScreen::default(),
            edit: EditScreen::default(),
        }
    }

    /// Rejects a job-triggering action while one is already in flight,
    /// otherwise marks a job as now running. Call right before returning an
    /// `Effect::Spawn` from an action handler.
    fn start_job(&mut self) -> bool {
        if self.job_running {
            self.status.error("Busy — a job is already running");
            return false;
        }
        self.job_running = true;
        true
    }

    /// Navigates to `screen`, remembering the current one so `go_back` can
    /// return to it.
    fn push_screen(&mut self, screen: Screen) {
        self.screen_stack.push(self.screen);
        self.screen = screen;
    }

    /// Returns to whichever screen `push_screen` was last called from;
    /// Library if the stack is already empty (there's nowhere further back).
    fn go_back(&mut self) {
        self.screen = self.screen_stack.pop().unwrap_or(Screen::Library);
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
                match crate::keymap::map_key(&self.screen, &self.mode, &mut self.pending, key) {
                    Some(action) => self.apply(action),
                    None => Vec::new(),
                }
            }
            Message::Input(_) => Vec::new(),
            Message::Job(_id, outcome) => self.apply_job_outcome(outcome),
            Message::ViewerExited(citation_key, result) => self.apply_viewer_exit(citation_key, result),
        }
    }

    fn apply(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::Quit => {
                self.should_quit = true;
                vec![Effect::Quit]
            }
            Action::MoveDown => {
                self.move_down();
                Vec::new()
            }
            Action::MoveUp => {
                self.move_up();
                Vec::new()
            }
            Action::GoTop => {
                self.go_top();
                Vec::new()
            }
            Action::GoBottom => {
                self.go_bottom();
                Vec::new()
            }
            Action::EnterInsert(target) => {
                match target {
                    InsertTarget::LibraryFilter => self.library.begin_filter_edit(),
                    InsertTarget::SearchQuery => self.search.begin_query_edit(),
                    edit_target => self.edit.begin_field_edit(edit_target),
                }
                self.mode = Mode::Insert(target);
                Vec::new()
            }
            Action::CancelInput => {
                self.mode = Mode::Normal;
                Vec::new()
            }
            Action::SubmitInput => self.submit_input(),
            Action::InputChar(c) => {
                self.push_input_char(c);
                Vec::new()
            }
            Action::InputBackspace => {
                self.pop_input_char();
                Vec::new()
            }
            Action::ClearFilter => {
                self.library.clear_filter();
                Vec::new()
            }
            Action::GoToSearch => {
                self.push_screen(Screen::Search);
                self.search.begin_query_edit();
                self.mode = Mode::Insert(InsertTarget::SearchQuery);
                Vec::new()
            }
            Action::Back => {
                self.go_back();
                Vec::new()
            }
            Action::CycleSearchMode => {
                self.search.cycle_kind();
                Vec::new()
            }
            Action::OpenDetail => {
                let subject = match self.screen {
                    Screen::Search => self
                        .search
                        .selected_candidate()
                        .map(|(work, in_library)| DetailSubject::Candidate { work, in_library }),
                    Screen::Library => self.library.selected_paper().map(DetailSubject::Declared),
                    Screen::Detail | Screen::Edit => None,
                };
                if let Some(subject) = subject {
                    self.detail.subject = Some(subject);
                    self.push_screen(Screen::Detail);
                }
                Vec::new()
            }
            Action::AddCandidate => self.add_selected_candidate(),
            Action::Fetch => self.fetch_selected(),
            Action::Open => self.open_selected(),
            Action::EnterEdit => self.enter_edit(),
            Action::SaveEdit => self.save_edit(),
            Action::EditFocusedField => {
                if self.screen == Screen::Edit
                    && let Some(target) = self.edit.focused_insert_target()
                {
                    self.edit.begin_field_edit(target);
                    self.mode = Mode::Insert(target);
                }
                Vec::new()
            }
            Action::RemoveLastTag => {
                if self.screen == Screen::Edit {
                    self.edit.remove_last_tag();
                }
                Vec::new()
            }
        }
    }

    fn add_selected_candidate(&mut self) -> Vec<Effect> {
        let Screen::Detail = self.screen else {
            return Vec::new();
        };
        let Some(DetailSubject::Candidate { work, .. }) = &self.detail.subject else {
            return Vec::new();
        };
        let candidate_id = work.id.clone();
        if !self.start_job() {
            return Vec::new();
        }
        vec![Effect::Spawn(JobKind::AddCandidate(candidate_id))]
    }

    /// The declared paper the current screen has selected — the Library's
    /// highlighted row, or the paper a Detail view is showing (`None` for a
    /// candidate detail, which isn't fetchable/openable/editable). Shared
    /// by fetch/open/edit, all reachable from either screen.
    fn selected_declared_paper(&self) -> Option<pax_core::Paper> {
        match self.screen {
            Screen::Library => self.library.selected_paper(),
            Screen::Detail => match &self.detail.subject {
                Some(DetailSubject::Declared(paper)) => Some(paper.clone()),
                _ => None,
            },
            Screen::Search | Screen::Edit => None,
        }
    }

    fn selected_declared_key(&self) -> Option<String> {
        self.selected_declared_paper().map(|p| p.local.citation_key)
    }

    fn enter_edit(&mut self) -> Vec<Effect> {
        if let Some(paper) = self.selected_declared_paper() {
            self.edit = EditScreen::from_paper(&paper);
            self.push_screen(Screen::Edit);
        }
        Vec::new()
    }

    fn save_edit(&mut self) -> Vec<Effect> {
        if self.screen != Screen::Edit {
            return Vec::new();
        }
        if !self.start_job() {
            return Vec::new();
        }
        let citation_key = self.edit.citation_key.clone();
        let edits = self.edit.build_edits();
        vec![Effect::Spawn(JobKind::EditPaper { citation_key, edits })]
    }

    fn fetch_selected(&mut self) -> Vec<Effect> {
        let Some(citation_key) = self.selected_declared_key() else {
            return Vec::new();
        };
        if !self.start_job() {
            return Vec::new();
        }
        self.status.pending(format!("Fetching {citation_key}…"));
        vec![Effect::Spawn(JobKind::FetchPaper(citation_key))]
    }

    fn open_selected(&mut self) -> Vec<Effect> {
        let Some(citation_key) = self.selected_declared_key() else {
            return Vec::new();
        };
        if !self.start_job() {
            return Vec::new();
        }
        self.status.pending(format!("Opening {citation_key}…"));
        vec![Effect::Spawn(JobKind::ResolveForOpen(citation_key))]
    }

    fn apply_viewer_exit(&mut self, citation_key: String, result: std::io::Result<std::process::ExitStatus>) -> Vec<Effect> {
        match result {
            Ok(status) if status.success() => self.status.message = None,
            Ok(status) => self.status.error(format!("{citation_key}: viewer exited with {status}")),
            Err(e) => self.status.error(format!("{citation_key}: failed to launch viewer: {e}")),
        }
        Vec::new()
    }

    fn move_down(&mut self) {
        match self.screen {
            Screen::Library => self.library.move_down(),
            Screen::Search => self.search.move_down(),
            Screen::Edit => self.edit.focus_next(),
            Screen::Detail => {}
        }
    }

    fn move_up(&mut self) {
        match self.screen {
            Screen::Library => self.library.move_up(),
            Screen::Search => self.search.move_up(),
            Screen::Edit => self.edit.focus_prev(),
            Screen::Detail => {}
        }
    }

    fn go_top(&mut self) {
        match self.screen {
            Screen::Library => self.library.go_top(),
            Screen::Search => self.search.go_top(),
            Screen::Edit => self.edit.go_top(),
            Screen::Detail => {}
        }
    }

    fn go_bottom(&mut self) {
        match self.screen {
            Screen::Library => self.library.go_bottom(),
            Screen::Search => self.search.go_bottom(),
            Screen::Edit => self.edit.go_bottom(),
            Screen::Detail => {}
        }
    }

    fn push_input_char(&mut self, c: char) {
        match self.mode {
            Mode::Insert(InsertTarget::LibraryFilter) => self.library.filter_buffer.push(c),
            Mode::Insert(InsertTarget::SearchQuery) => self.search.query_buffer.push(c),
            Mode::Insert(_) => self.edit.buffer.push(c),
            Mode::Normal => {}
        }
    }

    fn pop_input_char(&mut self) {
        match self.mode {
            Mode::Insert(InsertTarget::LibraryFilter) => {
                self.library.filter_buffer.pop();
            }
            Mode::Insert(InsertTarget::SearchQuery) => {
                self.search.query_buffer.pop();
            }
            Mode::Insert(_) => {
                self.edit.buffer.pop();
            }
            Mode::Normal => {}
        }
    }

    fn submit_input(&mut self) -> Vec<Effect> {
        let effects = match self.mode {
            Mode::Insert(InsertTarget::LibraryFilter) => {
                self.library.submit_filter_edit();
                Vec::new()
            }
            Mode::Insert(InsertTarget::SearchQuery) => self.submit_search_query(),
            Mode::Insert(target) => {
                self.edit.commit_field_edit(target);
                Vec::new()
            }
            Mode::Normal => Vec::new(),
        };
        self.mode = Mode::Normal;
        effects
    }

    fn submit_search_query(&mut self) -> Vec<Effect> {
        let query = self.search.submit_query_edit();
        if query.is_empty() {
            return Vec::new();
        }
        if !self.start_job() {
            return Vec::new();
        }
        self.search.state = SearchState::Loading;
        self.search.selected = 0;
        let job = match self.search.kind {
            SearchKind::Free => JobKind::SearchAll(query),
            SearchKind::Author => JobKind::SearchByAuthor(query),
            SearchKind::Doi => JobKind::SearchByDoi(query),
        };
        vec![Effect::Spawn(job)]
    }

    fn apply_job_outcome(&mut self, outcome: JobOutcome) -> Vec<Effect> {
        self.job_running = false;
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
            JobOutcome::Searched { results, known_dois } => {
                self.search.state = SearchState::Loaded { results, known_dois };
                self.search.selected = 0;
            }
            JobOutcome::Added(Ok(paper_ref)) => {
                self.status
                    .success(format!("Added {} — PDF not fetched yet", paper_ref.0));
                self.job_running = true;
                return vec![Effect::Spawn(JobKind::LoadLibrary)];
            }
            JobOutcome::Added(Err(e)) => {
                self.status.error(e.to_string());
            }
            JobOutcome::Fetched { citation_key, result } => match result {
                Ok(outcome) => {
                    let hash = match &outcome {
                        FetchOutcome::AlreadyFetched { hash } | FetchOutcome::Fetched { hash } => hash.clone(),
                    };
                    self.status.success(match &outcome {
                        FetchOutcome::Fetched { .. } => format!("Fetched {citation_key} (hash {hash})"),
                        FetchOutcome::AlreadyFetched { .. } => format!("{citation_key} already fetched"),
                    });
                    // Only patch the Detail screen's own copy if it's still
                    // showing the same paper — the user may have navigated
                    // to a different one while this job was in flight.
                    if let Some(DetailSubject::Declared(paper)) = &mut self.detail.subject
                        && paper.local.citation_key == citation_key
                    {
                        paper.artifact.hash = Some(hash);
                    }
                    self.job_running = true;
                    return vec![Effect::Spawn(JobKind::LoadLibrary)];
                }
                Err(e) => self.status.error(format!("{citation_key}: {e}")),
            },
            JobOutcome::ReadyToOpen { citation_key, result } => match result {
                Ok(path) => {
                    self.job_running = true;
                    return vec![
                        Effect::Spawn(JobKind::LoadLibrary),
                        Effect::LaunchViewer { citation_key, path },
                    ];
                }
                Err(e) => self.status.error(e.to_string()),
            },
            JobOutcome::Edited { citation_key, result } => match result {
                Ok(()) => {
                    self.status.success(format!("Updated {citation_key}"));
                    // A rename means `citation_key` no longer identifies the
                    // paper; rather than patch every screen's stale copy of
                    // it, just return to Library, where the reload below
                    // picks up the current state.
                    self.screen = Screen::Library;
                    self.screen_stack.clear();
                    self.job_running = true;
                    return vec![Effect::Spawn(JobKind::LoadLibrary)];
                }
                Err(e) => self.status.error(format!("{citation_key}: {e}")),
            },
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

    #[test]
    fn go_to_search_switches_screen_and_enters_insert_mode() {
        let mut app = App::new();
        app.apply(Action::GoToSearch);
        assert!(matches!(app.screen, Screen::Search));
        assert!(matches!(app.mode, Mode::Insert(InsertTarget::SearchQuery)));
    }

    #[test]
    fn back_returns_to_library_from_search() {
        let mut app = App::new();
        app.apply(Action::GoToSearch);
        app.apply(Action::Back);
        assert!(matches!(app.screen, Screen::Library));
    }

    #[test]
    fn submitting_an_empty_query_does_not_spawn_a_job() {
        let mut app = App::new();
        app.apply(Action::GoToSearch);
        let effects = app.apply(Action::SubmitInput);
        assert!(effects.is_empty());
        assert!(matches!(app.search.state, SearchState::Idle));
    }

    #[test]
    fn submitting_a_query_spawns_a_search_job_and_shows_loading() {
        let mut app = App::new();
        app.apply(Action::GoToSearch);
        for c in "actor model".chars() {
            app.apply(Action::InputChar(c));
        }
        let effects = app.apply(Action::SubmitInput);
        assert!(matches!(effects.as_slice(), [Effect::Spawn(JobKind::SearchAll(q))] if q == "actor model"));
        assert!(matches!(app.search.state, SearchState::Loading));
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn cycling_search_mode_changes_which_job_kind_submit_spawns() {
        let mut app = App::new();
        app.apply(Action::GoToSearch);
        app.apply(Action::CycleSearchMode); // Free -> Author
        for c in "hewitt".chars() {
            app.apply(Action::InputChar(c));
        }
        let effects = app.apply(Action::SubmitInput);
        assert!(matches!(effects.as_slice(), [Effect::Spawn(JobKind::SearchByAuthor(q))] if q == "hewitt"));
    }

    fn candidate_work() -> pax_core::CandidateWork {
        pax_core::CandidateWork {
            id: pax_core::CandidateId {
                provider: pax_core::ProviderId::OpenAlex,
                native_id: "W1".to_string(),
            },
            title: "On Computable Numbers".to_string(),
            authors: vec!["Alan Turing".to_string()],
            publish_date: "1936".to_string(),
            doi: None,
            pdf_url: None,
            venue: None,
            abstract_text: None,
        }
    }

    fn with_search_results() -> App {
        use std::collections::{HashMap, HashSet};
        let mut app = App::new();
        let mut results = HashMap::new();
        results.insert(pax_core::ProviderId::OpenAlex, Ok(vec![candidate_work()]));
        app.update(Message::Job(
            1,
            JobOutcome::Searched {
                results,
                known_dois: HashSet::new(),
            },
        ));
        app
    }

    #[test]
    fn open_detail_from_search_pushes_the_selected_candidate() {
        let mut app = with_search_results();
        app.screen = Screen::Search;
        app.apply(Action::OpenDetail);
        assert!(matches!(app.screen, Screen::Detail));
        assert!(matches!(
            app.detail.subject,
            Some(DetailSubject::Candidate { ref work, .. }) if work.title == "On Computable Numbers"
        ));
    }

    #[test]
    fn open_detail_is_a_no_op_when_nothing_is_loaded_yet() {
        let mut app = App::new();
        app.apply(Action::OpenDetail);
        assert!(matches!(app.screen, Screen::Library));
        assert!(app.detail.subject.is_none());
    }

    #[test]
    fn open_detail_from_library_shows_the_selected_declared_paper() {
        let mut app = with_two_papers();
        app.library.selected = 1; // hewitt1973
        app.apply(Action::OpenDetail);
        assert!(matches!(app.screen, Screen::Detail));
        assert!(matches!(
            app.detail.subject,
            Some(DetailSubject::Declared(ref p)) if p.local.citation_key == "hewitt1973"
        ));
    }

    #[test]
    fn back_from_declared_detail_returns_to_library() {
        let mut app = with_two_papers();
        app.apply(Action::OpenDetail);
        assert!(matches!(app.screen, Screen::Detail));
        app.apply(Action::Back);
        assert!(matches!(app.screen, Screen::Library));
    }

    #[test]
    fn back_from_detail_returns_to_search_not_library() {
        let mut app = with_search_results();
        app.apply(Action::GoToSearch); // Library -> Search (pushes Library)
        app.apply(Action::OpenDetail); // Search -> Detail (pushes Search)
        assert!(matches!(app.screen, Screen::Detail));
        app.apply(Action::Back);
        assert!(matches!(app.screen, Screen::Search));
        app.apply(Action::Back);
        assert!(matches!(app.screen, Screen::Library));
    }

    #[test]
    fn back_with_an_empty_stack_stays_on_library() {
        let mut app = App::new();
        app.apply(Action::Back);
        assert!(matches!(app.screen, Screen::Library));
    }

    #[test]
    fn search_results_populate_state_and_reset_selection() {
        use std::collections::{HashMap, HashSet};
        let mut app = App::new();
        app.search.selected = 5;
        app.update(Message::Job(
            1,
            JobOutcome::Searched {
                results: HashMap::new(),
                known_dois: HashSet::new(),
            },
        ));
        assert!(matches!(app.search.state, SearchState::Loaded { .. }));
        assert_eq!(app.search.selected, 0);
    }

    fn on_candidate_detail() -> App {
        let mut app = with_search_results();
        app.apply(Action::GoToSearch);
        app.apply(Action::OpenDetail);
        app
    }

    #[test]
    fn add_candidate_spawns_the_job_and_marks_one_in_flight() {
        let mut app = on_candidate_detail();
        let effects = app.apply(Action::AddCandidate);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::AddCandidate(id))] if id.native_id == "W1"
        ));
        assert!(app.job_running);
    }

    #[test]
    fn add_candidate_is_a_no_op_outside_detail() {
        let mut app = with_search_results();
        app.apply(Action::GoToSearch);
        let effects = app.apply(Action::AddCandidate);
        assert!(effects.is_empty());
        assert!(!app.job_running);
    }

    #[test]
    fn a_second_add_while_one_is_in_flight_is_rejected_with_a_status_message() {
        let mut app = on_candidate_detail();
        app.apply(Action::AddCandidate);
        let effects = app.apply(Action::AddCandidate);
        assert!(effects.is_empty());
        assert!(app.status.message.is_some());
    }

    #[test]
    fn successful_add_shows_success_and_triggers_a_library_reload() {
        let mut app = on_candidate_detail();
        app.apply(Action::AddCandidate);
        let effects = app.update(Message::Job(
            2,
            JobOutcome::Added(Ok(pax_core::PaperRef("turing1936".to_string()))),
        ));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Success, msg)) if msg.contains("turing1936")
        ));
        assert!(matches!(effects.as_slice(), [Effect::Spawn(JobKind::LoadLibrary)]));
        assert!(app.job_running); // the follow-up reload is itself now in flight
    }

    #[test]
    fn failed_add_shows_an_error_and_does_not_reload() {
        let mut app = on_candidate_detail();
        app.apply(Action::AddCandidate);
        let effects = app.update(Message::Job(
            2,
            JobOutcome::Added(Err(PaxError::NoSourceUrl("x".to_string()))),
        ));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, _))
        ));
        assert!(effects.is_empty());
        assert!(!app.job_running);
    }

    #[test]
    fn fetch_from_library_spawns_the_job_and_shows_pending() {
        let mut app = with_two_papers();
        app.library.selected = 0; // turing1936
        let effects = app.apply(Action::Fetch);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::FetchPaper(key))] if key == "turing1936"
        ));
        assert!(app.job_running);
        assert!(matches!(&app.status.message, Some((crate::status::StatusKind::Pending, _))));
    }

    #[test]
    fn fetch_from_declared_detail_targets_the_paper_being_viewed() {
        let mut app = with_two_papers();
        app.library.selected = 1; // hewitt1973
        app.apply(Action::OpenDetail);
        let effects = app.apply(Action::Fetch);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::FetchPaper(key))] if key == "hewitt1973"
        ));
    }

    #[test]
    fn fetch_and_open_are_no_ops_on_search_and_candidate_detail() {
        let mut app = on_candidate_detail();
        assert!(app.apply(Action::Fetch).is_empty());
        assert!(app.apply(Action::Open).is_empty());
        assert!(!app.job_running);
    }

    #[test]
    fn successful_fetch_updates_status_patches_matching_detail_and_reloads() {
        let mut app = with_two_papers();
        app.library.selected = 0; // turing1936
        app.apply(Action::OpenDetail);
        app.apply(Action::Fetch);
        let effects = app.update(Message::Job(
            2,
            JobOutcome::Fetched {
                citation_key: "turing1936".to_string(),
                result: Ok(FetchOutcome::Fetched {
                    hash: "sha256-abc".to_string(),
                }),
            },
        ));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Success, msg)) if msg.contains("turing1936")
        ));
        assert!(matches!(
            app.detail.subject,
            Some(DetailSubject::Declared(ref p)) if p.artifact.hash.as_deref() == Some("sha256-abc")
        ));
        assert!(matches!(effects.as_slice(), [Effect::Spawn(JobKind::LoadLibrary)]));
        assert!(app.job_running);
    }

    #[test]
    fn fetch_outcome_does_not_patch_detail_if_the_user_navigated_to_a_different_paper() {
        // Regression test: fetch was triggered for turing1936, but the user
        // has since opened hewitt1973's detail. The stale outcome must not
        // overwrite hewitt1973's artifact hash.
        let mut app = with_two_papers();
        app.library.selected = 1; // hewitt1973, viewed after the fetch was dispatched
        app.apply(Action::OpenDetail);
        app.update(Message::Job(
            2,
            JobOutcome::Fetched {
                citation_key: "turing1936".to_string(),
                result: Ok(FetchOutcome::Fetched {
                    hash: "sha256-abc".to_string(),
                }),
            },
        ));
        assert!(matches!(
            app.detail.subject,
            Some(DetailSubject::Declared(ref p)) if p.local.citation_key == "hewitt1973" && p.artifact.hash.is_none()
        ));
    }

    #[test]
    fn failed_fetch_shows_an_error() {
        let mut app = with_two_papers();
        app.apply(Action::Fetch);
        let effects = app.update(Message::Job(
            2,
            JobOutcome::Fetched {
                citation_key: "turing1936".to_string(),
                result: Err(PaxError::NoSourceUrl("turing1936".to_string())),
            },
        ));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, _))
        ));
        assert!(effects.is_empty());
        assert!(!app.job_running);
    }

    #[test]
    fn successful_open_reloads_and_launches_the_viewer() {
        let mut app = with_two_papers();
        app.apply(Action::Open);
        let effects = app.update(Message::Job(
            2,
            JobOutcome::ReadyToOpen {
                citation_key: "turing1936".to_string(),
                result: Ok(std::path::PathBuf::from("/nix/store/xyz-turing.pdf")),
            },
        ));
        assert!(matches!(effects.as_slice(), [Effect::Spawn(JobKind::LoadLibrary), Effect::LaunchViewer { .. }]));
        assert!(app.job_running);
    }

    #[test]
    fn failed_open_shows_an_error_and_does_not_launch_anything() {
        let mut app = with_two_papers();
        app.apply(Action::Open);
        let effects = app.update(Message::Job(
            2,
            JobOutcome::ReadyToOpen {
                citation_key: "turing1936".to_string(),
                result: Err(crate::job::OpenError::NoSourceUrl("turing1936".to_string())),
            },
        ));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, _))
        ));
        assert!(effects.is_empty());
        assert!(!app.job_running);
    }

    #[test]
    fn viewer_exiting_successfully_clears_the_status_message() {
        let mut app = App::new();
        app.status.pending("Opening turing1936…");
        app.update(Message::ViewerExited(
            "turing1936".to_string(),
            Ok(std::os::unix::process::ExitStatusExt::from_raw(0)),
        ));
        assert!(app.status.message.is_none());
    }

    #[test]
    fn viewer_failing_to_launch_shows_an_error() {
        let mut app = App::new();
        app.update(Message::ViewerExited(
            "turing1936".to_string(),
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no such viewer")),
        ));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, msg)) if msg.contains("turing1936")
        ));
    }

    #[test]
    fn enter_edit_from_library_prefills_from_the_selected_paper() {
        let mut app = with_two_papers();
        app.library.selected = 1; // hewitt1973
        app.apply(Action::EnterEdit);
        assert!(matches!(app.screen, Screen::Edit));
        assert_eq!(app.edit.citation_key, "hewitt1973");
        assert_eq!(app.edit.title, "A Universal Modular Actor Formalism");
    }

    #[test]
    fn enter_edit_is_a_no_op_on_search() {
        let mut app = with_search_results();
        app.apply(Action::GoToSearch);
        app.apply(Action::EnterEdit);
        assert!(matches!(app.screen, Screen::Search));
    }

    #[test]
    fn save_edit_spawns_the_job_with_the_built_edits() {
        let mut app = with_two_papers();
        app.apply(Action::EnterEdit); // turing1936
        app.edit.tags.push("newtag".to_string());
        let effects = app.apply(Action::SaveEdit);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::EditPaper { citation_key, edits })]
                if citation_key == "turing1936" && edits.add_tags == vec!["newtag".to_string()]
        ));
        assert!(app.job_running);
    }

    #[test]
    fn save_edit_is_a_no_op_outside_edit_screen() {
        let mut app = with_two_papers();
        let effects = app.apply(Action::SaveEdit);
        assert!(effects.is_empty());
        assert!(!app.job_running);
    }

    #[test]
    fn successful_edit_shows_success_returns_to_library_and_reloads() {
        let mut app = with_two_papers();
        app.apply(Action::EnterEdit);
        app.apply(Action::SaveEdit);
        let effects = app.update(Message::Job(
            2,
            JobOutcome::Edited {
                citation_key: "turing1936".to_string(),
                result: Ok(()),
            },
        ));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Success, msg)) if msg.contains("turing1936")
        ));
        assert!(matches!(app.screen, Screen::Library));
        assert!(app.screen_stack.is_empty());
        assert!(matches!(effects.as_slice(), [Effect::Spawn(JobKind::LoadLibrary)]));
        assert!(app.job_running);
    }

    #[test]
    fn failed_edit_shows_an_error_and_stays_on_the_edit_screen() {
        let mut app = with_two_papers();
        app.apply(Action::EnterEdit);
        app.apply(Action::SaveEdit);
        let effects = app.update(Message::Job(
            2,
            JobOutcome::Edited {
                citation_key: "turing1936".to_string(),
                result: Err(PaxError::NoChangesSpecified),
            },
        ));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, _))
        ));
        assert!(matches!(app.screen, Screen::Edit));
        assert!(effects.is_empty());
        assert!(!app.job_running);
    }

    #[test]
    fn edit_focused_field_enters_insert_mode_for_the_focused_field() {
        let mut app = with_two_papers();
        app.apply(Action::EnterEdit);
        app.edit.focus = crate::ui::edit::EditField::Title;
        app.apply(Action::EditFocusedField);
        assert!(matches!(app.mode, Mode::Insert(InsertTarget::EditTitle)));
        assert_eq!(app.edit.buffer, "On Computable Numbers");
    }

    #[test]
    fn edit_focused_field_on_tags_is_a_no_op_since_tags_has_no_single_buffer() {
        let mut app = with_two_papers();
        app.apply(Action::EnterEdit); // focus starts on Tags
        app.apply(Action::EditFocusedField);
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn typing_and_submitting_updates_the_edit_screens_field() {
        let mut app = with_two_papers();
        app.apply(Action::EnterEdit);
        app.edit.focus = crate::ui::edit::EditField::Notes;
        app.apply(Action::EditFocusedField);
        for c in "new notes".chars() {
            app.apply(Action::InputChar(c));
        }
        app.apply(Action::SubmitInput);
        assert_eq!(app.edit.notes, "new notes");
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn remove_last_tag_action_only_applies_on_edit_screen() {
        let mut app = with_two_papers();
        app.apply(Action::RemoveLastTag);
        assert!(app.library.selected_paper().is_some()); // untouched, still on Library

        app.apply(Action::EnterEdit);
        let before = app.edit.tags.len();
        app.apply(Action::RemoveLastTag);
        assert_eq!(app.edit.tags.len(), before.saturating_sub(1));
    }
}
