use std::io::ErrorKind;

use crossterm::event::{Event, KeyEventKind};
use pax_core::{ExclusionStage, FetchOutcome, Limit, LogOutcome, PaxError};

use crate::action::Action;
use crate::job::{ExcludeRequest, JobKind, JobOutcome};
use crate::message::{Effect, Message};
use crate::query::SearchRequest;
use crate::status::StatusBar;
use crate::ui::detail::{DetailScreen, DetailSubject};
use crate::ui::edit::EditScreen;
use crate::ui::export::ExportScreen;
use crate::ui::library::{LibraryScreen, LibraryState};
use crate::ui::search::{LoadedSearch, SearchScreen, SearchState};
use crate::ui::stats::{StatsScreen, StatsState};

/// Which top-level view is showing. Gains a variant per screen as each one
/// is built (see pax-project/.github's build-log.md).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Library,
    Search,
    Detail,
    Edit,
    SyncReport,
    CheckReport,
    Export,
    Stats,
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
    SearchFilters,
    EditTagAdd,
    EditNotes,
    EditRename,
    EditTitle,
    EditAuthors,
    EditYear,
    EditDoi,
    EditSourceUrl,
    UploadPdfPath,
    ExportPath,
    ExcludeReason,
}

/// Tracks a `g` press waiting for a second key (`gg` -> go to top). Reset on
/// any key that isn't the completing `g`.
#[derive(Default)]
pub struct PendingInput {
    pub g_pressed: bool,
}

/// A y/n confirmation overlay. `on_confirm` is re-dispatched through
/// `apply()` on `y`, exactly as if the user had triggered it directly —
/// the only action that currently reaches this is `Action::RemoveConfirmed`,
/// but nothing here is remove-specific.
pub struct ConfirmPrompt {
    pub message: String,
    pub on_confirm: Action,
}

/// The exclusion being composed: the paper `x` was pressed on, frozen at
/// that moment so moving the selection can't change the target, and the
/// reason and stage typed so far. Shown on the status line while
/// `Mode::Insert(InsertTarget::ExcludeReason)` is active.
pub struct ExcludePrompt {
    pub reference: String,
    pub title: String,
    pub reason: String,
    pub stage: ExclusionStage,
}

impl ExcludePrompt {
    fn toggle_stage(&mut self) {
        self.stage = match self.stage {
            ExclusionStage::Screening => ExclusionStage::FullText,
            _ => ExclusionStage::Screening,
        };
    }
}

pub struct App {
    pub screen: Screen,
    pub screen_stack: Vec<Screen>,
    pub mode: Mode,
    pub should_quit: bool,
    pub pending: PendingInput,
    pub confirm: Option<ConfirmPrompt>,
    pub exclude: Option<ExcludePrompt>,
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
    pub sync_report: Option<Vec<pax_core::SyncReport>>,
    pub sync_selected: usize,
    pub check_report: Option<Vec<pax_core::CheckReport>>,
    pub check_selected: usize,
    pub export: ExportScreen,
    pub stats: StatsScreen,
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Library,
            screen_stack: Vec::new(),
            mode: Mode::Normal,
            should_quit: false,
            pending: PendingInput::default(),
            confirm: None,
            exclude: None,
            status: StatusBar::default(),
            job_running: false,
            library: LibraryScreen::default(),
            search: SearchScreen::default(),
            detail: DetailScreen::default(),
            edit: EditScreen::default(),
            sync_report: None,
            sync_selected: 0,
            check_report: None,
            check_selected: 0,
            export: ExportScreen::default(),
            stats: StatsScreen::default(),
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
            Message::Tick => {
                self.status.clear_if_expired();
                Vec::new()
            }
            Message::Input(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                match crate::keymap::map_key(
                    &self.screen,
                    &self.mode,
                    self.confirm.is_some(),
                    &mut self.pending,
                    key,
                ) {
                    Some(action) => self.apply(action),
                    None => Vec::new(),
                }
            }
            Message::Input(_) => Vec::new(),
            Message::Job(_id, outcome) => self.apply_job_outcome(outcome),
            Message::ViewerExited(citation_key, result) => {
                self.apply_viewer_exit(citation_key, result)
            }
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
                    InsertTarget::SearchFilters => self.search.begin_filter_edit(),
                    InsertTarget::ExportPath => self.export.buffer = self.export.path.clone(),
                    // Opened only by `TriggerExclude`, which picks the paper.
                    InsertTarget::ExcludeReason => return Vec::new(),
                    edit_target => self.edit.begin_field_edit(edit_target),
                }
                self.mode = Mode::Insert(target);
                Vec::new()
            }
            Action::CancelInput => {
                self.mode = Mode::Normal;
                self.exclude = None;
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
                    Screen::Detail
                    | Screen::Edit
                    | Screen::SyncReport
                    | Screen::CheckReport
                    | Screen::Export
                    | Screen::Stats => None,
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
            Action::TriggerRemove => {
                if let Some(citation_key) = self.selected_declared_key() {
                    self.confirm = Some(ConfirmPrompt {
                        message: format!("Remove {citation_key}?"),
                        on_confirm: Action::RemoveConfirmed,
                    });
                }
                Vec::new()
            }
            Action::RemoveConfirmed => self.remove_selected(),
            Action::ConfirmYes => match self.confirm.take() {
                Some(prompt) => self.apply(prompt.on_confirm),
                None => Vec::new(),
            },
            Action::ConfirmNo => {
                self.confirm = None;
                Vec::new()
            }
            Action::TriggerSync => self.trigger_library_job(JobKind::Sync, "Syncing library…"),
            Action::TriggerCheck => self.trigger_library_job(JobKind::Check, "Checking library…"),
            Action::EnterExport => {
                if self.screen == Screen::Library {
                    // Pure/no I/O — pax_core::bibtex::render just formats a
                    // string from data already in memory, so it's called
                    // directly here rather than through a job, same as
                    // filter_papers.
                    let bibtex = pax_core::bibtex::render(self.library.declared_papers());
                    self.export = ExportScreen {
                        bibtex,
                        path: String::new(),
                        buffer: String::new(),
                    };
                    self.push_screen(Screen::Export);
                }
                Vec::new()
            }
            Action::SaveExport => self.save_export(),
            Action::TriggerInit => self.trigger_init(),
            Action::LoadMore => self.load_more(false),
            Action::LoadAll => self.load_more(true),
            Action::OpenStats => self.open_stats(),
            Action::TriggerExclude => {
                self.begin_exclude();
                Vec::new()
            }
            Action::CycleExclusionStage => {
                if let Some(prompt) = &mut self.exclude {
                    prompt.toggle_stage();
                }
                Vec::new()
            }
        }
    }

    /// The search result the current screen shows: the Search screen's
    /// highlighted row, or the candidate a Detail view is showing. Its
    /// representative record, the one `a` would add.
    fn selected_candidate_work(&self) -> Option<pax_core::CandidateWork> {
        match self.screen {
            Screen::Search => self.search.selected_candidate().map(|(work, _)| work),
            Screen::Detail => match &self.detail.subject {
                Some(DetailSubject::Candidate { work, .. }) => Some(work.clone()),
                _ => None,
            },
            Screen::Library
            | Screen::Edit
            | Screen::SyncReport
            | Screen::CheckReport
            | Screen::Export
            | Screen::Stats => None,
        }
    }

    /// Opens the exclusion prompt on the selected search result. A declared
    /// paper's detail has nothing to exclude: pax-core refuses to exclude a
    /// paper that's in the library.
    fn begin_exclude(&mut self) {
        let Some(work) = self.selected_candidate_work() else {
            return;
        };
        self.exclude = Some(ExcludePrompt {
            reference: work.id.to_string(),
            title: work.title,
            reason: String::new(),
            stage: ExclusionStage::Screening,
        });
        self.mode = Mode::Insert(InsertTarget::ExcludeReason);
    }

    /// Dispatches the exclusion the prompt describes. The reason is
    /// required; without one nothing is recorded.
    fn submit_exclusion(&mut self) -> Vec<Effect> {
        let Some(prompt) = self.exclude.take() else {
            return Vec::new();
        };
        let reason = prompt.reason.trim().to_string();
        if reason.is_empty() {
            self.status
                .error("An exclusion needs a reason — nothing was excluded");
            return Vec::new();
        }
        if !self.start_job() {
            return Vec::new();
        }
        self.status
            .pending(format!("Excluding {} ({})…", prompt.title, prompt.stage));
        vec![Effect::Spawn(JobKind::Exclude(ExcludeRequest {
            reference: prompt.reference,
            title: prompt.title,
            reason,
            stage: prompt.stage,
        }))]
    }

    fn trigger_init(&mut self) -> Vec<Effect> {
        if !matches!(self.library.state, LibraryState::NotInitialized) {
            return Vec::new();
        }
        if !self.start_job() {
            return Vec::new();
        }
        self.status.pending("Initializing library…");
        vec![Effect::Spawn(JobKind::Init)]
    }

    /// Opens the PRISMA stats screen and recomputes its counts from the
    /// library files every time, so it never shows numbers older than the
    /// moment it was opened.
    fn open_stats(&mut self) -> Vec<Effect> {
        if self.screen != Screen::Library {
            return Vec::new();
        }
        if !self.start_job() {
            return Vec::new();
        }
        self.stats = StatsScreen::default();
        self.push_screen(Screen::Stats);
        vec![Effect::Spawn(JobKind::Stats)]
    }

    fn save_export(&mut self) -> Vec<Effect> {
        if self.screen != Screen::Export {
            return Vec::new();
        }
        if self.export.path.is_empty() {
            self.status
                .error("No file path entered — press i/p to type one");
            return Vec::new();
        }
        if !self.start_job() {
            return Vec::new();
        }
        let path = std::path::PathBuf::from(&self.export.path);
        let content = self.export.bibtex.clone();
        self.status
            .pending(format!("Writing {}…", self.export.path));
        vec![Effect::Spawn(JobKind::ExportToFile { path, content })]
    }

    /// `Sync`/`Check` aren't scoped to a selected paper — they're always
    /// reachable from Library regardless of selection, unlike
    /// fetch/open/edit/remove.
    fn trigger_library_job(&mut self, kind: JobKind, pending_message: &str) -> Vec<Effect> {
        if self.screen != Screen::Library {
            return Vec::new();
        }
        if !self.start_job() {
            return Vec::new();
        }
        self.status.pending(pending_message);
        vec![Effect::Spawn(kind)]
    }

    fn remove_selected(&mut self) -> Vec<Effect> {
        let Some(citation_key) = self.selected_declared_key() else {
            return Vec::new();
        };
        if !self.start_job() {
            return Vec::new();
        }
        vec![Effect::Spawn(JobKind::RemovePaper(citation_key))]
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
            Screen::Search
            | Screen::Edit
            | Screen::SyncReport
            | Screen::CheckReport
            | Screen::Export
            | Screen::Stats => None,
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
        vec![Effect::Spawn(JobKind::EditPaper {
            citation_key,
            edits,
        })]
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

    fn apply_viewer_exit(
        &mut self,
        citation_key: String,
        result: std::io::Result<std::process::ExitStatus>,
    ) -> Vec<Effect> {
        match result {
            Ok(status) if status.success() => self.status.message = None,
            Ok(status) => self
                .status
                .error(format!("{citation_key}: viewer exited with {status}")),
            Err(e) => self
                .status
                .error(format!("{citation_key}: failed to launch viewer: {e}")),
        }
        Vec::new()
    }

    fn move_down(&mut self) {
        match self.screen {
            Screen::Library => self.library.move_down(),
            Screen::Search => self.search.move_down(),
            Screen::Edit => self.edit.focus_next(),
            Screen::SyncReport => {
                let len = self.sync_report.as_ref().map_or(0, Vec::len);
                move_index_down(&mut self.sync_selected, len);
            }
            Screen::CheckReport => {
                let len = self.check_report.as_ref().map_or(0, Vec::len);
                move_index_down(&mut self.check_selected, len);
            }
            Screen::Stats => self.stats.scroll_down(),
            Screen::Detail | Screen::Export => {}
        }
    }

    fn move_up(&mut self) {
        match self.screen {
            Screen::Library => self.library.move_up(),
            Screen::Search => self.search.move_up(),
            Screen::Edit => self.edit.focus_prev(),
            Screen::SyncReport => {
                let len = self.sync_report.as_ref().map_or(0, Vec::len);
                move_index_up(&mut self.sync_selected, len);
            }
            Screen::CheckReport => {
                let len = self.check_report.as_ref().map_or(0, Vec::len);
                move_index_up(&mut self.check_selected, len);
            }
            Screen::Stats => self.stats.scroll_up(),
            Screen::Detail | Screen::Export => {}
        }
    }

    fn go_top(&mut self) {
        match self.screen {
            Screen::Library => self.library.go_top(),
            Screen::Search => self.search.go_top(),
            Screen::Edit => self.edit.go_top(),
            Screen::SyncReport => self.sync_selected = 0,
            Screen::CheckReport => self.check_selected = 0,
            Screen::Stats => self.stats.scroll = 0,
            Screen::Detail | Screen::Export => {}
        }
    }

    fn go_bottom(&mut self) {
        match self.screen {
            Screen::Library => self.library.go_bottom(),
            Screen::Search => self.search.go_bottom(),
            Screen::Edit => self.edit.go_bottom(),
            Screen::SyncReport => {
                let len = self.sync_report.as_ref().map_or(0, Vec::len);
                self.sync_selected = len.saturating_sub(1);
            }
            Screen::CheckReport => {
                let len = self.check_report.as_ref().map_or(0, Vec::len);
                self.check_selected = len.saturating_sub(1);
            }
            Screen::Stats => self.stats.scroll_bottom(),
            Screen::Detail | Screen::Export => {}
        }
    }

    fn push_input_char(&mut self, c: char) {
        match self.mode {
            Mode::Insert(InsertTarget::LibraryFilter) => self.library.filter_buffer.push(c),
            Mode::Insert(InsertTarget::SearchQuery) => self.search.query_buffer.push(c),
            Mode::Insert(InsertTarget::SearchFilters) => self.search.filter_buffer.push(c),
            Mode::Insert(InsertTarget::ExportPath) => self.export.buffer.push(c),
            Mode::Insert(InsertTarget::ExcludeReason) => {
                if let Some(prompt) = &mut self.exclude {
                    prompt.reason.push(c);
                }
            }
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
            Mode::Insert(InsertTarget::SearchFilters) => {
                self.search.filter_buffer.pop();
            }
            Mode::Insert(InsertTarget::ExportPath) => {
                self.export.buffer.pop();
            }
            Mode::Insert(InsertTarget::ExcludeReason) => {
                if let Some(prompt) = &mut self.exclude {
                    prompt.reason.pop();
                }
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
            Mode::Insert(InsertTarget::SearchFilters) => self.submit_search_filters(),
            Mode::Insert(InsertTarget::ExportPath) => {
                self.export.path = self.export.buffer.clone();
                self.export.buffer.clear();
                Vec::new()
            }
            Mode::Insert(InsertTarget::UploadPdfPath) => self.submit_upload_pdf_path(),
            Mode::Insert(InsertTarget::ExcludeReason) => self.submit_exclusion(),
            Mode::Insert(target) => {
                self.edit.commit_field_edit(target);
                Vec::new()
            }
            Mode::Normal => Vec::new(),
        };
        self.mode = Mode::Normal;
        effects
    }

    /// The path just typed into the Edit screen's "upload PDF" prompt —
    /// dispatches an upload job rather than writing into an `EditScreen`
    /// field directly, since a local path isn't itself a value to persist,
    /// only the resulting release URL is.
    fn submit_upload_pdf_path(&mut self) -> Vec<Effect> {
        let path = self.edit.buffer.trim().to_string();
        self.edit.buffer.clear();
        if path.is_empty() {
            self.status.error("No file path entered");
            return Vec::new();
        }
        if !self.start_job() {
            return Vec::new();
        }
        let citation_key = self.edit.citation_key.clone();
        self.status.pending(format!("Uploading {path} to GitHub…"));
        vec![Effect::Spawn(JobKind::UploadPdf {
            citation_key,
            local_path: std::path::PathBuf::from(path),
        })]
    }

    fn submit_search_query(&mut self) -> Vec<Effect> {
        let query = self.search.submit_query_edit();
        if query.is_empty() {
            return Vec::new();
        }
        self.run_search(self.search.request(query))
    }

    /// Applies the filter prompt. A malformed prompt leaves the filters as
    /// they were; valid filters re-run the current query, if there's one
    /// they apply to, so the results always match the filters shown.
    fn submit_search_filters(&mut self) -> Vec<Effect> {
        if let Err(e) = self.search.submit_filter_edit() {
            self.status.error(e);
            return Vec::new();
        }
        let request = self.search.request(self.search.query.clone());
        if request.query.is_empty() || !request.is_paged() {
            self.status
                .success("Filters set; they apply to the next search");
            return Vec::new();
        }
        self.run_search(request)
    }

    fn run_search(&mut self, request: SearchRequest) -> Vec<Effect> {
        if !self.start_job() {
            return Vec::new();
        }
        self.search.state = SearchState::Loading(request.clone());
        self.search.selected = 0;
        vec![Effect::Spawn(JobKind::Search(request))]
    }

    /// Re-runs the showing search with a larger per-provider limit — doubled
    /// (`m`), or every result (`M`). pax-core's paged search has no offset to
    /// continue from, so this is a fresh, logged search; the current results
    /// stay on screen until it returns.
    fn load_more(&mut self, all: bool) -> Vec<Effect> {
        if self.screen != Screen::Search {
            return Vec::new();
        }
        let Some(loaded) = self.search.loaded() else {
            return Vec::new();
        };
        if !loaded.request.is_paged() {
            self.status.error(
                "A DOI lookup resolves one record per provider; there's nothing more to load",
            );
            return Vec::new();
        }
        if !loaded.has_more() {
            self.status
                .success("Every provider's results are already loaded");
            return Vec::new();
        }
        let next = if all {
            loaded.request.all()
        } else {
            loaded.request.more()
        };
        let Some(next) = next else {
            self.status
                .success("Already retrieving every result each provider serves");
            return Vec::new();
        };
        if !self.start_job() {
            return Vec::new();
        }
        self.status.pending(match next.options.limit {
            Limit::Count(n) => format!("Loading up to {n} results per provider…"),
            _ => "Loading every result each provider serves (this can be slow)…".to_string(),
        });
        vec![Effect::Spawn(JobKind::Search(next))]
    }

    /// Surfaces a search that wasn't recorded in `research/search-log.jsonl`,
    /// the way `pax search` warns about it on stderr. Never fatal: the
    /// results are good either way.
    fn report_search_log(&mut self, log: LogOutcome) {
        match log {
            LogOutcome::NoLibrary => self
                .status
                .warning("Not in a pax library, so this search isn't recorded in a search log"),
            LogOutcome::Failed(e) => self.status.error(format!(
                "Couldn't write research/search-log.jsonl: {e} (the results are still good)"
            )),
            _ => {}
        }
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
            JobOutcome::Searched {
                request,
                results,
                index,
                log,
            } => {
                self.search
                    .show_results(LoadedSearch::new(*request, results, *index));
                self.status.clear_pending();
                if let Some(log) = log {
                    self.report_search_log(log);
                }
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
            JobOutcome::Fetched {
                citation_key,
                result,
            } => match result {
                Ok(outcome) => {
                    let hash = match &outcome {
                        FetchOutcome::AlreadyFetched { hash } | FetchOutcome::Fetched { hash } => {
                            hash.clone()
                        }
                    };
                    self.status.success(match &outcome {
                        FetchOutcome::Fetched { .. } => {
                            format!("Fetched {citation_key} (hash {hash})")
                        }
                        FetchOutcome::AlreadyFetched { .. } => {
                            format!("{citation_key} already fetched")
                        }
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
            JobOutcome::ReadyToOpen {
                citation_key,
                result,
            } => match result {
                Ok(path) => {
                    self.job_running = true;
                    return vec![
                        Effect::Spawn(JobKind::LoadLibrary),
                        Effect::LaunchViewer { citation_key, path },
                    ];
                }
                Err(e) => self.status.error(e.to_string()),
            },
            JobOutcome::Edited {
                citation_key,
                result,
            } => match result {
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
            JobOutcome::Removed {
                citation_key,
                result,
            } => match result {
                Ok(()) => {
                    self.status.success(format!("Removed {citation_key}"));
                    self.screen = Screen::Library;
                    self.screen_stack.clear();
                    self.job_running = true;
                    return vec![Effect::Spawn(JobKind::LoadLibrary)];
                }
                Err(e) => self.status.error(format!("{citation_key}: {e}")),
            },
            JobOutcome::Synced(Ok(reports)) => {
                self.status
                    .success(format!("Synced {} paper(s)", reports.len()));
                self.sync_selected = 0;
                self.sync_report = Some(reports);
                self.push_screen(Screen::SyncReport);
                self.job_running = true;
                return vec![Effect::Spawn(JobKind::LoadLibrary)];
            }
            JobOutcome::Synced(Err(e)) => self.status.error(e.to_string()),
            JobOutcome::Checked(Ok(reports)) => {
                self.status
                    .success(format!("Checked {} paper(s)", reports.len()));
                self.check_selected = 0;
                self.check_report = Some(reports);
                self.push_screen(Screen::CheckReport);
            }
            JobOutcome::Checked(Err(e)) => self.status.error(e.to_string()),
            JobOutcome::Exported { path, result } => match result {
                Ok(()) => self
                    .status
                    .success(format!("Exported to {}", path.display())),
                Err(e) => self.status.error(format!("{}: {e}", path.display())),
            },
            JobOutcome::Initialized(Ok(())) => {
                self.status.success("Library initialized");
                self.job_running = true;
                return vec![Effect::Spawn(JobKind::LoadLibrary)];
            }
            JobOutcome::Initialized(Err(e)) => self.status.error(e.to_string()),
            JobOutcome::Stats(Ok(stats)) => self.stats.state = StatsState::Loaded(stats),
            JobOutcome::Stats(Err(e)) => {
                self.status.error(format!("stats: {e}"));
                self.stats.state = StatsState::Failed(e.to_string());
            }
            JobOutcome::Uploaded {
                citation_key,
                result,
            } => match result {
                Ok(url) => {
                    self.status
                        .success(format!("Uploaded {citation_key}.pdf — source set"));
                    // Only patch the Edit screen's own copy if it's still
                    // showing the same paper — the user may have navigated
                    // away while this job was in flight (same guard used
                    // for a successful fetch's detail patch).
                    if self.screen == Screen::Edit && self.edit.citation_key == citation_key {
                        self.edit.apply_uploaded_source_url(url);
                    }
                    self.job_running = true;
                    return vec![Effect::Spawn(JobKind::LoadLibrary)];
                }
                Err(e) => self.status.error(format!("{citation_key}: {e}")),
            },
            JobOutcome::Excluded { reference, result } => match result {
                Ok((exclusion, index)) => {
                    self.search.replace_index(*index);
                    self.status.success(format!(
                        "Excluded {} ({}): {}",
                        exclusion.title, exclusion.stage, exclusion.reason
                    ));
                }
                // `pax exclude` asks for `--title` here; lazypax always
                // passes the result's title, so only an untitled result
                // with an unreachable provider ends up here.
                Err(PaxError::ExclusionTitleUnresolved(_, detail)) => self.status.error(format!(
                    "Couldn't exclude {reference}: the result has no title and its provider couldn't supply one ({detail})"
                )),
                Err(e) => self
                    .status
                    .error(format!("Couldn't exclude {reference}: {e}")),
            },
        }
        Vec::new()
    }
}

fn move_index_down(selected: &mut usize, len: usize) {
    if len > 0 {
        *selected = (*selected + 1) % len;
    }
}

fn move_index_up(selected: &mut usize, len: usize) {
    if len > 0 {
        *selected = (*selected + len - 1) % len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::SearchKind;

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
        app.update(Message::Job(
            1,
            JobOutcome::Library(Err(PaxError::Io(io_err))),
        ));
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
        assert!(matches!(
            app.mode,
            Mode::Insert(InsertTarget::LibraryFilter)
        ));
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
        assert!(
            matches!(effects.as_slice(), [Effect::Spawn(JobKind::Search(r))] if r.query == "actor model" && r.kind == SearchKind::Free)
        );
        assert!(matches!(app.search.state, SearchState::Loading(_)));
        assert!(matches!(app.mode, Mode::Normal));
    }

    fn type_filters(app: &mut App, text: &str) -> Vec<Effect> {
        app.apply(Action::EnterInsert(InsertTarget::SearchFilters));
        for c in text.chars() {
            app.apply(Action::InputChar(c));
        }
        app.apply(Action::SubmitInput)
    }

    #[test]
    fn filters_set_before_a_search_apply_to_it() {
        let mut app = App::new();
        app.apply(Action::GoToSearch);
        app.apply(Action::CancelInput);
        assert!(type_filters(&mut app, "year:2020 lang:en").is_empty());
        app.apply(Action::EnterInsert(InsertTarget::SearchQuery));
        for c in "actors".chars() {
            app.apply(Action::InputChar(c));
        }
        let effects = app.apply(Action::SubmitInput);
        let [Effect::Spawn(JobKind::Search(request))] = effects.as_slice() else {
            panic!("expected one search job");
        };
        assert_eq!(
            crate::query::format_filters(&request.options.filters),
            "year:2020 lang:en"
        );
    }

    #[test]
    fn changing_filters_re_runs_the_current_query() {
        let mut app = App::new();
        app.apply(Action::GoToSearch);
        for c in "actors".chars() {
            app.apply(Action::InputChar(c));
        }
        app.apply(Action::SubmitInput);
        app.job_running = false; // the first search finished
        let effects = type_filters(&mut app, "type:preprint");
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::Search(r))]
                if r.query == "actors" && r.options.filters.doc_type == Some(pax_core::DocType::Preprint)
        ));
    }

    #[test]
    fn a_malformed_filter_is_an_error_and_keeps_the_filters() {
        let mut app = App::new();
        app.apply(Action::GoToSearch);
        app.apply(Action::CancelInput);
        type_filters(&mut app, "year:2020");
        let effects = type_filters(&mut app, "year:20");
        assert!(effects.is_empty());
        assert_eq!(
            crate::query::format_filters(&app.search.filters),
            "year:2020"
        );
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, _))
        ));
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
        assert!(
            matches!(effects.as_slice(), [Effect::Spawn(JobKind::Search(r))] if r.query == "hewitt" && r.kind == SearchKind::Author)
        );
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
        use std::collections::HashMap;
        let mut app = App::new();
        let mut results = HashMap::new();
        results.insert(
            pax_core::ProviderId::OpenAlex,
            Ok(pax_core::SearchPage::new(
                vec![candidate_work()],
                None,
                None,
            )),
        );
        app.update(Message::Job(
            1,
            JobOutcome::Searched {
                request: Box::new(SearchRequest::new(SearchKind::Free, "q")),
                results,
                index: Box::default(),
                log: None,
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
        use std::collections::HashMap;
        let mut app = App::new();
        app.search.selected = 5;
        app.update(Message::Job(
            1,
            JobOutcome::Searched {
                request: Box::new(SearchRequest::new(SearchKind::Free, "q")),
                results: HashMap::new(),
                index: Box::default(),
                log: None,
            },
        ));
        assert!(matches!(app.search.state, SearchState::Loaded(_)));
        assert_eq!(app.search.selected, 0);
    }

    fn searched_with_log(log: Option<LogOutcome>) -> App {
        let mut app = App::new();
        app.update(Message::Job(
            1,
            JobOutcome::Searched {
                request: Box::new(SearchRequest::new(SearchKind::Free, "q")),
                results: std::collections::HashMap::new(),
                index: Box::default(),
                log,
            },
        ));
        app
    }

    #[test]
    fn a_logged_search_shows_no_status_message() {
        let app = searched_with_log(Some(LogOutcome::Written(
            "research/search-log.jsonl".into(),
        )));
        assert!(app.status.message.is_none());
    }

    #[test]
    fn a_search_outside_a_library_warns_that_it_was_not_logged() {
        let app = searched_with_log(Some(LogOutcome::NoLibrary));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Warning, msg)) if msg.contains("isn't recorded")
        ));
        assert!(matches!(app.search.state, SearchState::Loaded(_)));
    }

    #[test]
    fn a_failed_log_write_is_an_error_but_keeps_the_results() {
        let app = searched_with_log(Some(LogOutcome::Failed(std::io::Error::other("disk full"))));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, msg)) if msg.contains("disk full")
        ));
        assert!(matches!(app.search.state, SearchState::Loaded(_)));
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
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::LoadLibrary)]
        ));
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
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Pending, _))
        ));
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
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::LoadLibrary)]
        ));
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
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Spawn(JobKind::LoadLibrary),
                Effect::LaunchViewer { .. }
            ]
        ));
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
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no such viewer",
            )),
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
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::LoadLibrary)]
        ));
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
    fn submitting_an_empty_upload_path_shows_an_error_and_spawns_nothing() {
        let mut app = with_two_papers();
        app.apply(Action::EnterEdit);
        app.apply(Action::EnterInsert(InsertTarget::UploadPdfPath));
        let effects = app.apply(Action::SubmitInput);
        assert!(effects.is_empty());
        assert!(!app.job_running);
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, _))
        ));
    }

    #[test]
    fn entering_a_path_and_submitting_spawns_the_upload_job() {
        let mut app = with_two_papers();
        app.library.selected = 1; // hewitt1973
        app.apply(Action::EnterEdit);
        app.apply(Action::EnterInsert(InsertTarget::UploadPdfPath));
        for c in "/home/me/hewitt1973.pdf".chars() {
            app.apply(Action::InputChar(c));
        }
        let effects = app.apply(Action::SubmitInput);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::UploadPdf { citation_key, local_path })]
                if citation_key == "hewitt1973" && local_path == std::path::Path::new("/home/me/hewitt1973.pdf")
        ));
        assert!(app.job_running);
        assert!(matches!(app.mode, Mode::Normal));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Pending, _))
        ));
    }

    #[test]
    fn successful_upload_shows_success_patches_the_edit_screen_and_reloads() {
        let mut app = with_two_papers();
        app.apply(Action::EnterEdit); // turing1936
        app.apply(Action::EnterInsert(InsertTarget::UploadPdfPath));
        for c in "/home/me/turing.pdf".chars() {
            app.apply(Action::InputChar(c));
        }
        app.apply(Action::SubmitInput);
        let effects = app.update(Message::Job(
            2,
            JobOutcome::Uploaded {
                citation_key: "turing1936".to_string(),
                result: Ok(
                    "https://github.com/x/y/releases/download/papers/turing1936.pdf".to_string(),
                ),
            },
        ));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Success, msg)) if msg.contains("turing1936")
        ));
        assert_eq!(
            app.edit.source_url,
            "https://github.com/x/y/releases/download/papers/turing1936.pdf"
        );
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::LoadLibrary)]
        ));
        assert!(app.job_running);
    }

    #[test]
    fn successful_upload_does_not_patch_a_different_papers_edit_screen() {
        // Regression guard, same shape as fetch's: the upload was triggered
        // for turing1936, but the user has since opened hewitt1973's edit
        // screen. The stale outcome must not overwrite hewitt1973's fields.
        let mut app = with_two_papers();
        app.apply(Action::EnterEdit); // turing1936
        app.apply(Action::EnterInsert(InsertTarget::UploadPdfPath));
        for c in "/home/me/turing.pdf".chars() {
            app.apply(Action::InputChar(c));
        }
        app.apply(Action::SubmitInput);

        app.apply(Action::Back);
        app.library.selected = 1; // hewitt1973
        app.apply(Action::EnterEdit);

        app.update(Message::Job(
            2,
            JobOutcome::Uploaded {
                citation_key: "turing1936".to_string(),
                result: Ok(
                    "https://github.com/x/y/releases/download/papers/turing1936.pdf".to_string(),
                ),
            },
        ));
        assert_eq!(app.edit.citation_key, "hewitt1973");
        assert_eq!(app.edit.source_url, "");
    }

    #[test]
    fn failed_upload_shows_an_error() {
        let mut app = with_two_papers();
        app.apply(Action::EnterEdit);
        app.apply(Action::EnterInsert(InsertTarget::UploadPdfPath));
        for c in "/home/me/turing.pdf".chars() {
            app.apply(Action::InputChar(c));
        }
        app.apply(Action::SubmitInput);
        let effects = app.update(Message::Job(
            2,
            JobOutcome::Uploaded {
                citation_key: "turing1936".to_string(),
                result: Err(PaxError::Upload("gh: not authenticated".to_string())),
            },
        ));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, msg)) if msg.contains("turing1936")
        ));
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

    #[test]
    fn trigger_remove_opens_a_confirm_prompt_without_removing_anything() {
        let mut app = with_two_papers();
        app.library.selected = 0; // turing1936
        let effects = app.apply(Action::TriggerRemove);
        assert!(effects.is_empty());
        assert!(!app.job_running);
        assert!(matches!(
            &app.confirm,
            Some(prompt) if prompt.message.contains("turing1936")
        ));
    }

    #[test]
    fn trigger_remove_is_a_no_op_on_search() {
        let mut app = with_search_results();
        app.apply(Action::GoToSearch);
        app.apply(Action::TriggerRemove);
        assert!(app.confirm.is_none());
    }

    #[test]
    fn confirm_no_dismisses_the_prompt_without_spawning_a_job() {
        let mut app = with_two_papers();
        app.apply(Action::TriggerRemove);
        assert!(app.confirm.is_some());
        let effects = app.apply(Action::ConfirmNo);
        assert!(app.confirm.is_none());
        assert!(effects.is_empty());
        assert!(!app.job_running);
    }

    #[test]
    fn confirm_yes_dispatches_the_remove_job_for_the_confirmed_paper() {
        let mut app = with_two_papers();
        app.library.selected = 1; // hewitt1973
        app.apply(Action::TriggerRemove);
        let effects = app.apply(Action::ConfirmYes);
        assert!(app.confirm.is_none());
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::RemovePaper(key))] if key == "hewitt1973"
        ));
        assert!(app.job_running);
    }

    #[test]
    fn confirm_yes_with_no_pending_prompt_is_a_no_op() {
        let mut app = with_two_papers();
        let effects = app.apply(Action::ConfirmYes);
        assert!(effects.is_empty());
        assert!(!app.job_running);
    }

    #[test]
    fn keymap_not_app_is_what_keeps_the_confirmed_target_from_drifting() {
        // ConfirmYes re-resolves "the selected paper" at confirm time
        // rather than freezing it in the prompt - safe only because
        // keymap.rs's confirm_active check stops any other action
        // (including navigation) from reaching App::apply() while a
        // prompt is up. App::apply() itself has no such guard, so calling
        // it directly (bypassing keymap, as real input never would while
        // confirming) does let the target drift - this documents that the
        // safety property lives in keymap, not here, for anyone tempted to
        // add a second path into App::update() that skips it.
        let mut app = with_two_papers();
        app.library.selected = 0; // turing1936
        app.apply(Action::TriggerRemove);
        app.apply(Action::MoveDown); // only reachable by bypassing keymap
        let effects = app.apply(Action::ConfirmYes);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::RemovePaper(key))] if key == "hewitt1973"
        ));
    }

    #[test]
    fn successful_remove_shows_success_returns_to_library_and_reloads() {
        let mut app = with_two_papers();
        app.apply(Action::TriggerRemove);
        app.apply(Action::ConfirmYes);
        let effects = app.update(Message::Job(
            2,
            JobOutcome::Removed {
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
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::LoadLibrary)]
        ));
        assert!(app.job_running);
    }

    #[test]
    fn failed_remove_shows_an_error() {
        let mut app = with_two_papers();
        app.apply(Action::TriggerRemove);
        app.apply(Action::ConfirmYes);
        let effects = app.update(Message::Job(
            2,
            JobOutcome::Removed {
                citation_key: "turing1936".to_string(),
                result: Err(PaxError::NoSuchPaper("turing1936".to_string())),
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
    fn trigger_sync_and_check_only_apply_on_library() {
        let mut app = with_two_papers();
        let sync_effects = app.apply(Action::TriggerSync);
        assert!(matches!(
            sync_effects.as_slice(),
            [Effect::Spawn(JobKind::Sync)]
        ));
        assert!(app.job_running);
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Pending, _))
        ));

        app.job_running = false;
        let check_effects = app.apply(Action::TriggerCheck);
        assert!(matches!(
            check_effects.as_slice(),
            [Effect::Spawn(JobKind::Check)]
        ));

        app.job_running = false;
        app.apply(Action::GoToSearch);
        assert!(app.apply(Action::TriggerSync).is_empty());
        assert!(app.apply(Action::TriggerCheck).is_empty());
    }

    #[test]
    fn successful_sync_shows_reports_pushes_the_screen_and_reloads() {
        let mut app = with_two_papers();
        app.apply(Action::TriggerSync);
        let reports = vec![
            pax_core::SyncReport {
                citation_key: "turing1936".to_string(),
                result: Ok(FetchOutcome::Fetched {
                    hash: "sha256-abc".to_string(),
                }),
            },
            pax_core::SyncReport {
                citation_key: "hewitt1973".to_string(),
                result: Err(PaxError::NoSourceUrl("hewitt1973".to_string())),
            },
        ];
        let effects = app.update(Message::Job(2, JobOutcome::Synced(Ok(reports))));
        assert!(matches!(app.screen, Screen::SyncReport));
        assert_eq!(app.sync_report.as_ref().unwrap().len(), 2);
        assert_eq!(app.sync_selected, 0);
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Success, msg)) if msg.contains('2')
        ));
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::LoadLibrary)]
        ));
        assert!(app.job_running);
    }

    #[test]
    fn failed_sync_shows_an_error_and_does_not_push_a_screen() {
        let mut app = with_two_papers();
        app.apply(Action::TriggerSync);
        app.update(Message::Job(
            2,
            JobOutcome::Synced(Err(PaxError::NoChangesSpecified)),
        ));
        assert!(matches!(app.screen, Screen::Library));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, _))
        ));
    }

    #[test]
    fn successful_check_shows_reports_and_pushes_the_screen_without_reloading() {
        let mut app = with_two_papers();
        app.apply(Action::TriggerCheck);
        let reports = vec![pax_core::CheckReport {
            citation_key: "turing1936".to_string(),
            status: pax_core::CheckStatus::Reproducible,
        }];
        let effects = app.update(Message::Job(2, JobOutcome::Checked(Ok(reports))));
        assert!(matches!(app.screen, Screen::CheckReport));
        assert_eq!(app.check_report.as_ref().unwrap().len(), 1);
        assert!(effects.is_empty()); // check never writes, so no reload needed
        assert!(!app.job_running);
    }

    #[test]
    fn failed_check_shows_an_error() {
        let mut app = with_two_papers();
        app.apply(Action::TriggerCheck);
        app.update(Message::Job(
            2,
            JobOutcome::Checked(Err(PaxError::NoChangesSpecified)),
        ));
        assert!(matches!(app.screen, Screen::Library));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, _))
        ));
    }

    #[test]
    fn sync_and_check_report_screens_support_jk_navigation() {
        let mut app = with_two_papers();
        app.sync_report = Some(vec![
            pax_core::SyncReport {
                citation_key: "a".to_string(),
                result: Ok(FetchOutcome::AlreadyFetched {
                    hash: "h".to_string(),
                }),
            },
            pax_core::SyncReport {
                citation_key: "b".to_string(),
                result: Ok(FetchOutcome::AlreadyFetched {
                    hash: "h".to_string(),
                }),
            },
        ]);
        app.screen = Screen::SyncReport;
        app.apply(Action::MoveDown);
        assert_eq!(app.sync_selected, 1);
        app.apply(Action::MoveDown); // wraps
        assert_eq!(app.sync_selected, 0);
        app.apply(Action::MoveUp); // wraps the other way
        assert_eq!(app.sync_selected, 1);
    }

    #[test]
    fn navigation_on_an_empty_report_is_a_no_op() {
        let mut app = App::new();
        app.screen = Screen::SyncReport;
        app.apply(Action::MoveDown);
        assert_eq!(app.sync_selected, 0);
    }

    #[test]
    fn back_from_sync_report_returns_to_library() {
        let mut app = with_two_papers();
        app.apply(Action::TriggerSync);
        app.update(Message::Job(2, JobOutcome::Synced(Ok(Vec::new()))));
        assert!(matches!(app.screen, Screen::SyncReport));
        app.apply(Action::Back);
        assert!(matches!(app.screen, Screen::Library));
    }

    #[test]
    fn enter_export_renders_bibtex_from_every_declared_paper_ignoring_the_filter() {
        let mut app = with_two_papers();
        app.library.query = "hewitt".to_string(); // filtered view would be just one paper
        app.apply(Action::EnterExport);
        assert!(matches!(app.screen, Screen::Export));
        assert!(app.export.bibtex.contains("turing1936"));
        assert!(app.export.bibtex.contains("hewitt1973"));
        assert_eq!(app.export.path, "");
    }

    #[test]
    fn enter_export_is_a_no_op_outside_library() {
        let mut app = with_search_results();
        app.apply(Action::GoToSearch);
        app.apply(Action::EnterExport);
        assert!(matches!(app.screen, Screen::Search));
    }

    #[test]
    fn save_export_without_a_path_shows_an_error_and_spawns_nothing() {
        let mut app = with_two_papers();
        app.apply(Action::EnterExport);
        let effects = app.apply(Action::SaveExport);
        assert!(effects.is_empty());
        assert!(!app.job_running);
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, _))
        ));
    }

    #[test]
    fn typing_a_path_and_submitting_commits_it_then_save_export_spawns_the_job() {
        let mut app = with_two_papers();
        app.apply(Action::EnterExport);
        app.apply(Action::EnterInsert(InsertTarget::ExportPath));
        for c in "out.bib".chars() {
            app.apply(Action::InputChar(c));
        }
        app.apply(Action::SubmitInput);
        assert_eq!(app.export.path, "out.bib");
        assert!(matches!(app.mode, Mode::Normal));

        let effects = app.apply(Action::SaveExport);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::ExportToFile { path, .. })] if path.to_str() == Some("out.bib")
        ));
        assert!(app.job_running);
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Pending, _))
        ));
    }

    #[test]
    fn successful_export_shows_the_written_path() {
        let mut app = with_two_papers();
        app.apply(Action::EnterExport);
        app.export.path = "out.bib".to_string();
        app.apply(Action::SaveExport);
        app.update(Message::Job(
            2,
            JobOutcome::Exported {
                path: std::path::PathBuf::from("out.bib"),
                result: Ok(()),
            },
        ));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Success, msg)) if msg.contains("out.bib")
        ));
        assert!(!app.job_running);
    }

    #[test]
    fn failed_export_shows_the_io_error() {
        let mut app = with_two_papers();
        app.apply(Action::EnterExport);
        app.export.path = "/root/no-permission.bib".to_string();
        app.apply(Action::SaveExport);
        app.update(Message::Job(
            2,
            JobOutcome::Exported {
                path: std::path::PathBuf::from("/root/no-permission.bib"),
                result: Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "denied",
                )),
            },
        ));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, msg)) if msg.contains("no-permission.bib")
        ));
    }

    fn not_initialized() -> App {
        let mut app = App::new();
        let io_err = std::io::Error::new(ErrorKind::NotFound, "no such file");
        app.update(Message::Job(
            1,
            JobOutcome::Library(Err(PaxError::Io(io_err))),
        ));
        app
    }

    #[test]
    fn trigger_init_spawns_the_job_only_when_not_initialized() {
        let mut app = not_initialized();
        let effects = app.apply(Action::TriggerInit);
        assert!(matches!(effects.as_slice(), [Effect::Spawn(JobKind::Init)]));
        assert!(app.job_running);
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Pending, _))
        ));
    }

    #[test]
    fn trigger_init_is_a_no_op_once_a_library_is_already_loaded() {
        let mut app = with_two_papers();
        let effects = app.apply(Action::TriggerInit);
        assert!(effects.is_empty());
        assert!(!app.job_running);
    }

    #[test]
    fn successful_init_shows_success_and_reloads_the_library() {
        let mut app = not_initialized();
        app.apply(Action::TriggerInit);
        let effects = app.update(Message::Job(2, JobOutcome::Initialized(Ok(()))));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Success, _))
        ));
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::LoadLibrary)]
        ));
        assert!(app.job_running);
    }

    #[test]
    fn failed_init_shows_the_io_error() {
        let mut app = not_initialized();
        app.apply(Action::TriggerInit);
        let effects = app.update(Message::Job(
            2,
            JobOutcome::Initialized(Err(std::io::Error::new(
                ErrorKind::AlreadyExists,
                "research/ already exists",
            ))),
        ));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, _))
        ));
        assert!(effects.is_empty());
        assert!(!app.job_running);
    }

    #[test]
    fn opening_stats_from_library_pushes_the_screen_and_spawns_the_job() {
        let mut app = with_two_papers();
        let effects = app.apply(Action::OpenStats);
        assert!(matches!(app.screen, Screen::Stats));
        assert!(matches!(app.stats.state, StatsState::Loading));
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::Stats)]
        ));
        assert!(app.job_running);
    }

    #[test]
    fn opening_stats_is_library_only_and_respects_a_running_job() {
        let mut app = App::new();
        app.apply(Action::GoToSearch);
        assert!(app.apply(Action::OpenStats).is_empty());
        assert!(matches!(app.screen, Screen::Search));

        let mut app = with_two_papers();
        app.job_running = true;
        assert!(app.apply(Action::OpenStats).is_empty());
        assert!(matches!(app.screen, Screen::Library));
    }

    #[test]
    fn stats_outcome_loads_the_counts() {
        let mut app = with_two_papers();
        app.apply(Action::OpenStats);
        app.update(Message::Job(
            2,
            JobOutcome::Stats(Ok(pax_core::Stats::default())),
        ));
        assert!(matches!(app.stats.state, StatsState::Loaded(_)));
        assert!(!app.job_running);
        assert!(app.status.message.is_none());
    }

    #[test]
    fn a_stats_error_shows_in_the_screen_and_the_status_line() {
        let mut app = with_two_papers();
        app.apply(Action::OpenStats);
        app.update(Message::Job(
            2,
            JobOutcome::Stats(Err(PaxError::NoSuchPaper("x".to_string()))),
        ));
        assert!(matches!(app.stats.state, StatsState::Failed(_)));
        assert!(matches!(app.screen, Screen::Stats));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, _))
        ));
        assert!(!app.job_running);
    }

    #[test]
    fn reopening_stats_recomputes_instead_of_showing_stale_counts() {
        let mut app = with_two_papers();
        app.apply(Action::OpenStats);
        app.update(Message::Job(
            2,
            JobOutcome::Stats(Ok(pax_core::Stats::default())),
        ));
        app.stats.scroll = 3;
        app.apply(Action::Back);
        assert!(matches!(app.screen, Screen::Library));
        let effects = app.apply(Action::OpenStats);
        assert!(matches!(app.stats.state, StatsState::Loading));
        assert_eq!(app.stats.scroll, 0);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Spawn(JobKind::Stats)]
        ));
    }

    /// The Search screen with one result (`openalex:W1`, "On Computable
    /// Numbers") highlighted.
    fn on_search_results() -> App {
        let mut app = with_search_results();
        app.apply(Action::GoToSearch);
        app.apply(Action::CancelInput);
        app
    }

    fn type_reason(app: &mut App, reason: &str) {
        for c in reason.chars() {
            app.apply(Action::InputChar(c));
        }
    }

    fn excluded_index() -> pax_core::LibraryIndex {
        let mut exclusions = pax_core::Exclusions::default();
        exclusions.upsert(pax_core::Exclusion::new(
            "openalex:W1".to_string(),
            "On Computable Numbers".to_string(),
            "off topic".to_string(),
            ExclusionStage::Screening,
        ));
        pax_core::LibraryIndex::new(&pax_core::Library::new(Vec::new()), &exclusions)
    }

    #[test]
    fn x_on_a_search_result_opens_the_reason_prompt_at_the_screening_stage() {
        let mut app = on_search_results();
        let effects = app.apply(Action::TriggerExclude);
        assert!(effects.is_empty());
        assert!(matches!(
            app.mode,
            Mode::Insert(InsertTarget::ExcludeReason)
        ));
        let prompt = app.exclude.as_ref().unwrap();
        assert_eq!(prompt.reference, "openalex:W1");
        assert_eq!(prompt.title, "On Computable Numbers");
        assert_eq!(prompt.stage, ExclusionStage::Screening);
        assert!(prompt.reason.is_empty());
    }

    #[test]
    fn x_on_a_candidate_detail_opens_the_prompt_for_that_candidate() {
        let mut app = on_candidate_detail();
        app.apply(Action::TriggerExclude);
        assert!(matches!(
            &app.exclude,
            Some(prompt) if prompt.reference == "openalex:W1"
        ));
    }

    #[test]
    fn x_does_nothing_on_a_declared_paper_or_before_any_results() {
        let mut app = with_two_papers();
        app.apply(Action::OpenDetail); // turing1936's declared detail
        app.apply(Action::TriggerExclude);
        assert!(app.exclude.is_none());
        assert!(matches!(app.mode, Mode::Normal));

        let mut app = App::new();
        app.apply(Action::GoToSearch);
        app.apply(Action::CancelInput);
        app.apply(Action::TriggerExclude);
        assert!(app.exclude.is_none());
    }

    #[test]
    fn tab_toggles_the_stage_between_screening_and_full_text() {
        let mut app = on_search_results();
        app.apply(Action::TriggerExclude);
        app.apply(Action::CycleExclusionStage);
        assert_eq!(
            app.exclude.as_ref().unwrap().stage,
            ExclusionStage::FullText
        );
        app.apply(Action::CycleExclusionStage);
        assert_eq!(
            app.exclude.as_ref().unwrap().stage,
            ExclusionStage::Screening
        );
    }

    #[test]
    fn submitting_a_reason_dispatches_the_exclusion_with_the_rows_reference_and_title() {
        let mut app = on_search_results();
        app.apply(Action::TriggerExclude);
        type_reason(&mut app, "  not empirical x");
        app.apply(Action::InputBackspace);
        app.apply(Action::InputBackspace);
        app.apply(Action::CycleExclusionStage);
        let effects = app.apply(Action::SubmitInput);
        let [Effect::Spawn(JobKind::Exclude(request))] = effects.as_slice() else {
            panic!("expected one exclude job");
        };
        assert_eq!(
            *request,
            ExcludeRequest {
                reference: "openalex:W1".to_string(),
                title: "On Computable Numbers".to_string(),
                reason: "not empirical".to_string(),
                stage: ExclusionStage::FullText,
            }
        );
        assert!(app.job_running);
        assert!(app.exclude.is_none());
        assert!(matches!(app.mode, Mode::Normal));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Pending, _))
        ));
    }

    #[test]
    fn an_empty_reason_is_an_error_and_excludes_nothing() {
        let mut app = on_search_results();
        app.apply(Action::TriggerExclude);
        type_reason(&mut app, "   ");
        let effects = app.apply(Action::SubmitInput);
        assert!(effects.is_empty());
        assert!(!app.job_running);
        assert!(app.exclude.is_none());
        assert!(matches!(app.mode, Mode::Normal));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, msg)) if msg.contains("reason")
        ));
    }

    #[test]
    fn esc_cancels_the_exclusion_prompt() {
        let mut app = on_search_results();
        app.apply(Action::TriggerExclude);
        type_reason(&mut app, "off topic");
        let effects = app.apply(Action::CancelInput);
        assert!(effects.is_empty());
        assert!(app.exclude.is_none());
        assert!(matches!(app.mode, Mode::Normal));
        assert!(!app.job_running);
    }

    #[test]
    fn an_exclusion_while_another_job_runs_is_rejected() {
        let mut app = on_search_results();
        app.job_running = true;
        app.apply(Action::TriggerExclude);
        type_reason(&mut app, "off topic");
        assert!(app.apply(Action::SubmitInput).is_empty());
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, msg)) if msg.contains("Busy")
        ));
    }

    #[test]
    fn a_successful_exclusion_swaps_in_the_reloaded_index_and_flags_the_row() {
        let mut app = on_search_results();
        app.apply(Action::TriggerExclude);
        type_reason(&mut app, "off topic");
        app.apply(Action::SubmitInput);
        assert!(!crate::ui::search::tests::render(&app.search).contains("[excluded]"));

        let exclusion = pax_core::Exclusion::new(
            "openalex:W1".to_string(),
            "On Computable Numbers".to_string(),
            "off topic".to_string(),
            ExclusionStage::Screening,
        );
        let effects = app.update(Message::Job(
            2,
            JobOutcome::Excluded {
                reference: "openalex:W1".to_string(),
                result: Ok((exclusion, Box::new(excluded_index()))),
            },
        ));
        assert!(effects.is_empty());
        assert!(!app.job_running);
        assert!(
            crate::ui::search::tests::render(&app.search)
                .contains("On Computable Numbers [excluded]")
        );
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Success, msg))
                if msg == "Excluded On Computable Numbers (screening): off topic"
        ));
    }

    #[test]
    fn a_refused_exclusion_shows_the_error_and_keeps_the_flags() {
        let mut app = on_search_results();
        app.apply(Action::TriggerExclude);
        type_reason(&mut app, "off topic");
        app.apply(Action::SubmitInput);
        app.update(Message::Job(
            2,
            JobOutcome::Excluded {
                reference: "openalex:W1".to_string(),
                result: Err(PaxError::ExcludingDeclaredPaper(
                    "openalex:W1".to_string(),
                    "turing1936".to_string(),
                )),
            },
        ));
        assert!(!app.job_running);
        assert!(!crate::ui::search::tests::render(&app.search).contains("[excluded]"));
        assert!(matches!(
            &app.status.message,
            Some((crate::status::StatusKind::Error, msg))
                if msg.contains("openalex:W1") && msg.contains("turing1936")
        ));
    }
}
