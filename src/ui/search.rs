use std::collections::{HashMap, HashSet};

use pax_core::{CandidateWork, ProviderError, ProviderId};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::ui::SELECTED_ROW_STYLE;

/// Fixed iteration order shared with the `pax` CLI's own `search`/`show`
/// commands, so results render in the same provider order a `pax` user
/// already expects.
const PROVIDER_ORDER: [ProviderId; 4] = [
    ProviderId::OpenAlex,
    ProviderId::Crossref,
    ProviderId::SemanticScholar,
    ProviderId::ArXiv,
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Free,
    Author,
    Doi,
}

impl SearchKind {
    fn label(self) -> &'static str {
        match self {
            SearchKind::Free => "free",
            SearchKind::Author => "author",
            SearchKind::Doi => "doi",
        }
    }

    fn next(self) -> Self {
        match self {
            SearchKind::Free => SearchKind::Author,
            SearchKind::Author => SearchKind::Doi,
            SearchKind::Doi => SearchKind::Free,
        }
    }
}

pub enum SearchState {
    Idle,
    Loading,
    Loaded {
        results: HashMap<ProviderId, Result<Vec<CandidateWork>, ProviderError>>,
        known_dois: HashSet<String>,
    },
}

pub struct SearchScreen {
    pub kind: SearchKind,
    /// The committed, already-searched query.
    pub query: String,
    /// Live edit buffer while `Mode::Insert(InsertTarget::SearchQuery)` is
    /// active; only copied into `query` on submit.
    pub query_buffer: String,
    pub state: SearchState,
    pub selected: usize,
}

impl Default for SearchScreen {
    fn default() -> Self {
        Self {
            kind: SearchKind::Free,
            query: String::new(),
            query_buffer: String::new(),
            state: SearchState::Idle,
            selected: 0,
        }
    }
}

impl SearchScreen {
    fn candidates(&self) -> Option<Vec<CandidateWork>> {
        match &self.state {
            SearchState::Loaded { results, .. } => Some(flatten(results)),
            _ => None,
        }
    }

    /// The currently-highlighted candidate and whether it's already
    /// declared in the library, for opening a detail view on it.
    pub fn selected_candidate(&self) -> Option<(CandidateWork, bool)> {
        let (results, known_dois) = match &self.state {
            SearchState::Loaded { results, known_dois } => (results, known_dois),
            _ => return None,
        };
        let work = flatten(results).get(self.selected).cloned()?;
        let in_library = work
            .doi
            .as_deref()
            .is_some_and(|d| known_dois.contains(pax_core::normalize_doi(d)));
        Some((work, in_library))
    }

    pub fn move_down(&mut self) {
        if let Some(items) = self.candidates().filter(|c| !c.is_empty()) {
            self.selected = (self.selected + 1) % items.len();
        }
    }

    pub fn move_up(&mut self) {
        if let Some(items) = self.candidates().filter(|c| !c.is_empty()) {
            self.selected = (self.selected + items.len() - 1) % items.len();
        }
    }

    pub fn go_top(&mut self) {
        self.selected = 0;
    }

    pub fn go_bottom(&mut self) {
        if let Some(items) = self.candidates().filter(|c| !c.is_empty()) {
            self.selected = items.len() - 1;
        }
    }

    pub fn cycle_kind(&mut self) {
        self.kind = self.kind.next();
    }

    pub fn begin_query_edit(&mut self) {
        self.query_buffer = self.query.clone();
    }

    /// Commits the edit buffer as the new query and returns it (trimmed),
    /// ready for the caller to decide whether to actually dispatch a search.
    pub fn submit_query_edit(&mut self) -> String {
        self.query = self.query_buffer.trim().to_string();
        self.query.clone()
    }
}

fn flatten(results: &HashMap<ProviderId, Result<Vec<CandidateWork>, ProviderError>>) -> Vec<CandidateWork> {
    PROVIDER_ORDER
        .iter()
        .filter_map(|p| results.get(p).and_then(|r| r.as_ref().ok()))
        .flat_map(|works| works.iter().cloned())
        .collect()
}

fn provider_errors(
    results: &HashMap<ProviderId, Result<Vec<CandidateWork>, ProviderError>>,
) -> Vec<(ProviderId, String)> {
    PROVIDER_ORDER
        .iter()
        .filter_map(|p| results.get(p).and_then(|r| r.as_ref().err().map(|e| (*p, e.to_string()))))
        .collect()
}

pub fn draw(frame: &mut Frame, screen: &SearchScreen, editing: bool, area: Rect) {
    let header = header_text(screen, editing);
    let header_height = header.lines().count() as u16;
    let [header_area, list_area] =
        Layout::vertical([Constraint::Length(header_height), Constraint::Min(0)]).areas(area);

    frame.render_widget(Paragraph::new(header), header_area);

    match &screen.state {
        SearchState::Idle => render_message(frame, list_area, "Type a query and press Enter to search."),
        SearchState::Loading => render_message(frame, list_area, "Searching…"),
        SearchState::Loaded { results, known_dois } => {
            let items = flatten(results);
            if items.is_empty() {
                render_message(frame, list_area, "No results.");
            } else {
                render_list(frame, &items, known_dois, screen.selected, list_area);
            }
        }
    }
}

fn header_text(screen: &SearchScreen, editing: bool) -> String {
    let text = if editing { &screen.query_buffer } else { &screen.query };
    let cursor = if editing { "▏" } else { "" };
    let mut header = format!(
        "Search [{}] (Tab: cycle mode, Enter: search, /: edit, Esc: back): {text}{cursor}",
        screen.kind.label()
    );
    if let SearchState::Loaded { results, .. } = &screen.state {
        for (provider, message) in provider_errors(results) {
            header.push('\n');
            header.push_str(&format!("{provider}: failed — {message}"));
        }
    }
    header
}

fn render_message(frame: &mut Frame, area: Rect, message: &str) {
    let block = Block::default().title(" lazypax — search ").borders(Borders::ALL);
    frame.render_widget(Paragraph::new(message).block(block), area);
}

fn render_list(frame: &mut Frame, items: &[CandidateWork], known_dois: &HashSet<String>, selected: usize, area: Rect) {
    let list_items: Vec<ListItem> = items
        .iter()
        .map(|work| ListItem::new(format_candidate(work, known_dois)))
        .collect();
    let list = List::new(list_items)
        .block(Block::default().title(" lazypax — search ").borders(Borders::ALL))
        .highlight_style(SELECTED_ROW_STYLE);
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn format_candidate(work: &CandidateWork, known_dois: &HashSet<String>) -> String {
    let authors = if work.authors.is_empty() {
        "—".to_string()
    } else {
        work.authors.join(", ")
    };
    let year: String = work.publish_date.chars().take(4).collect();
    let venue = work.venue.as_deref().unwrap_or("no venue");
    let doi = work.doi.as_deref().unwrap_or("no doi");
    let pdf = if work.pdf_url.is_some() { "✓" } else { "✗" };
    let in_library = work
        .doi
        .as_deref()
        .is_some_and(|d| known_dois.contains(pax_core::normalize_doi(d)));

    format!(
        "[{}] {}{}\n    {authors} · {year} · {venue} · DOI: {doi} · PDF: {pdf}",
        work.id.provider,
        work.title,
        if in_library { " [in library]" } else { "" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pax_core::CandidateId;

    fn work(provider: ProviderId, native_id: &str, title: &str) -> CandidateWork {
        CandidateWork {
            id: CandidateId {
                provider,
                native_id: native_id.to_string(),
            },
            title: title.to_string(),
            authors: vec![],
            publish_date: "2020".to_string(),
            doi: None,
            pdf_url: None,
            venue: None,
            abstract_text: None,
        }
    }

    fn loaded(results: HashMap<ProviderId, Result<Vec<CandidateWork>, ProviderError>>) -> SearchScreen {
        SearchScreen {
            state: SearchState::Loaded {
                results,
                known_dois: HashSet::new(),
            },
            ..Default::default()
        }
    }

    #[test]
    fn flatten_preserves_provider_order_and_skips_errors() {
        let mut results: HashMap<ProviderId, Result<Vec<CandidateWork>, ProviderError>> = HashMap::new();
        results.insert(ProviderId::Crossref, Ok(vec![work(ProviderId::Crossref, "1", "B")]));
        results.insert(ProviderId::OpenAlex, Ok(vec![work(ProviderId::OpenAlex, "2", "A")]));
        results.insert(ProviderId::SemanticScholar, Err(ProviderError::NotFound));

        let flat = flatten(&results);
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0].title, "A"); // OpenAlex before Crossref, per PROVIDER_ORDER
        assert_eq!(flat[1].title, "B");
    }

    #[test]
    fn provider_errors_lists_only_failing_providers() {
        let mut results: HashMap<ProviderId, Result<Vec<CandidateWork>, ProviderError>> = HashMap::new();
        results.insert(ProviderId::OpenAlex, Ok(vec![]));
        results.insert(ProviderId::Crossref, Err(ProviderError::NotFound));

        let errors = provider_errors(&results);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, ProviderId::Crossref);
    }

    #[test]
    fn movement_wraps_across_flattened_results() {
        let mut results: HashMap<ProviderId, Result<Vec<CandidateWork>, ProviderError>> = HashMap::new();
        results.insert(
            ProviderId::OpenAlex,
            Ok(vec![work(ProviderId::OpenAlex, "1", "A"), work(ProviderId::OpenAlex, "2", "B")]),
        );
        let mut screen = loaded(results);
        screen.selected = 1;
        screen.move_down();
        assert_eq!(screen.selected, 0);
        screen.move_up();
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn cycle_kind_rotates_free_author_doi() {
        let mut screen = SearchScreen::default();
        assert!(matches!(screen.kind, SearchKind::Free));
        screen.cycle_kind();
        assert!(matches!(screen.kind, SearchKind::Author));
        screen.cycle_kind();
        assert!(matches!(screen.kind, SearchKind::Doi));
        screen.cycle_kind();
        assert!(matches!(screen.kind, SearchKind::Free));
    }

    #[test]
    fn selected_candidate_reports_in_library_from_normalized_doi() {
        let mut w = work(ProviderId::OpenAlex, "1", "A");
        w.doi = Some("https://doi.org/10.1145/foo".to_string());
        let mut results: HashMap<ProviderId, Result<Vec<CandidateWork>, ProviderError>> = HashMap::new();
        results.insert(ProviderId::OpenAlex, Ok(vec![w]));
        let mut known_dois = HashSet::new();
        known_dois.insert("10.1145/foo".to_string());
        let screen = SearchScreen {
            state: SearchState::Loaded { results, known_dois },
            ..Default::default()
        };
        let (candidate, in_library) = screen.selected_candidate().unwrap();
        assert_eq!(candidate.title, "A");
        assert!(in_library);
    }

    #[test]
    fn selected_candidate_is_none_before_a_search_completes() {
        assert!(SearchScreen::default().selected_candidate().is_none());
    }

    #[test]
    fn submit_query_edit_trims_and_commits() {
        let mut screen = SearchScreen {
            query_buffer: "  actor model  ".to_string(),
            ..Default::default()
        };
        let committed = screen.submit_query_edit();
        assert_eq!(committed, "actor model");
        assert_eq!(screen.query, "actor model");
    }
}
