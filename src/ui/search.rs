use pax_core::{
    CandidateWork, FilterApplication, Filters, LibraryIndex, Limit, MergeReport, MergedWork,
    ProviderId, SearchPage,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::InsertTarget;
use crate::job::{ProviderFailure, SearchResults};
use crate::query::{SearchKind, SearchRequest, format_filters, parse_filters};
use crate::ui::{SELECTED_ROW_STYLE, wrapped_height};

/// Fixed iteration order shared with the `pax` CLI's own `search`/`show`
/// commands, so results render in the same provider order a `pax` user
/// already expects.
const PROVIDER_ORDER: [ProviderId; 4] = [
    ProviderId::OpenAlex,
    ProviderId::Crossref,
    ProviderId::SemanticScholar,
    ProviderId::ArXiv,
];

/// A finished search: the request that produced it, each provider's page
/// (or error), the library index its results are checked against, and the
/// results merged across providers into one row per paper, as `pax search`
/// lists them.
pub struct LoadedSearch {
    pub request: SearchRequest,
    pub results: SearchResults,
    pub index: LibraryIndex,
    pub merged: MergeReport,
}

impl LoadedSearch {
    pub fn new(request: SearchRequest, results: SearchResults, index: LibraryIndex) -> Self {
        let merged = pax_core::merge_pages(&results);
        LoadedSearch {
            request,
            results,
            index,
            merged,
        }
    }

    /// One row per paper, its records from different providers merged.
    pub fn rows(&self) -> &[MergedWork] {
        &self.merged.works
    }

    /// A row's flags: in library or excluded if any of its records is, and
    /// language unknown only if every record is, as `pax search` marks it.
    fn marks(&self, row: &MergedWork) -> Marks {
        let unknown = |work: &CandidateWork| {
            self.results
                .values()
                .flatten()
                .any(|page| page.is_language_unknown(work))
        };
        Marks {
            in_library: row.records().any(|work| self.index.contains(work)),
            excluded: row.records().any(|work| self.index.is_excluded(work)),
            language_unknown: row.records().all(unknown),
        }
    }

    /// Whether loading more could retrieve anything: the search pages, and
    /// some provider reported more matches than were retrieved and isn't
    /// already at the most it serves for one query.
    pub fn has_more(&self) -> bool {
        self.request.is_paged()
            && self
                .results
                .values()
                .flatten()
                .any(|page| !page.is_complete() && page.total.is_some() && !at_ceiling(page))
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Marks {
    in_library: bool,
    excluded: bool,
    language_unknown: bool,
}

/// Whether `page` stopped at the provider's per-query ceiling (e.g.
/// Semantic Scholar's 1,000), where asking for more can't help.
fn at_ceiling(page: &SearchPage) -> bool {
    page.max_retrievable
        .is_some_and(|max| page.scanned.max(page.retrieved() as u64) >= max)
}

pub enum SearchState {
    Idle,
    /// A new search is in flight; nothing to show yet.
    Loading(SearchRequest),
    Loaded(Box<LoadedSearch>),
}

pub struct SearchScreen {
    pub kind: SearchKind,
    /// The committed, already-searched query.
    pub query: String,
    /// Live edit buffer while `Mode::Insert(InsertTarget::SearchQuery)` is
    /// active; only copied into `query` on submit.
    pub query_buffer: String,
    /// The year / type / language filters every new search applies.
    pub filters: Filters,
    /// Live edit buffer while `Mode::Insert(InsertTarget::SearchFilters)` is
    /// active; parsed into `filters` on submit.
    pub filter_buffer: String,
    pub state: SearchState,
    pub selected: usize,
}

impl Default for SearchScreen {
    fn default() -> Self {
        Self {
            kind: SearchKind::Free,
            query: String::new(),
            query_buffer: String::new(),
            filters: Filters::default(),
            filter_buffer: String::new(),
            state: SearchState::Idle,
            selected: 0,
        }
    }
}

impl SearchScreen {
    /// Opens the filter prompt with the filters already set.
    pub fn begin_filter_edit(&mut self) {
        self.filter_buffer = format_filters(&self.filters);
    }

    /// Parses the filter prompt into `filters`. On an error the filters stay
    /// as they were and the error is returned for the status line.
    pub fn submit_filter_edit(&mut self) -> Result<(), String> {
        self.filters = parse_filters(&self.filter_buffer)?;
        Ok(())
    }

    /// A new search for `query` of the current kind, with the current
    /// filters.
    pub fn request(&self, query: String) -> SearchRequest {
        SearchRequest::new(self.kind, query).with_filters(self.filters.clone())
    }

    /// The search to re-run when the filters change: the citation list
    /// showing, if it is one, else the committed query. Either starts again
    /// at the default limit, as a new search does.
    pub fn refiltered_request(&self) -> SearchRequest {
        match self.current_request() {
            Some(current) if current.kind.is_citations() => SearchRequest {
                options: Default::default(),
                ..current.clone()
            }
            .with_filters(self.filters.clone()),
            _ => self.request(self.query.clone()),
        }
    }

    pub fn loaded(&self) -> Option<&LoadedSearch> {
        match &self.state {
            SearchState::Loaded(loaded) => Some(loaded),
            _ => None,
        }
    }

    /// The search showing or loading, if any.
    pub fn current_request(&self) -> Option<&SearchRequest> {
        match &self.state {
            SearchState::Idle => None,
            SearchState::Loading(request) => Some(request),
            SearchState::Loaded(loaded) => Some(&loaded.request),
        }
    }

    /// Whether `request` is the search showing or loading, perhaps with a
    /// different limit or filters — the same query or citation list.
    pub fn is_showing(&self, request: &SearchRequest) -> bool {
        self.current_request()
            .is_some_and(|current| current.kind == request.kind && current.query == request.query)
    }

    fn row_count(&self) -> usize {
        self.loaded().map_or(0, |loaded| loaded.rows().len())
    }

    /// Shows `loaded`. A load-more of the search already showing keeps the
    /// selection where it was; a new search starts at the top.
    pub fn show_results(&mut self, loaded: LoadedSearch) {
        let same_search = self.loaded().is_some() && self.is_showing(&loaded.request);
        self.state = SearchState::Loaded(Box::new(loaded));
        if same_search {
            self.selected = self.selected.min(self.row_count().saturating_sub(1));
        } else {
            self.selected = 0;
        }
    }

    /// Swaps in a freshly loaded library index — after an exclusion, so the
    /// rows' flags show what `papers.nix` and `excluded.nix` now say without
    /// re-running the search.
    pub fn replace_index(&mut self, index: LibraryIndex) {
        if let SearchState::Loaded(loaded) = &mut self.state {
            loaded.index = index;
        }
    }

    /// The highlighted row's representative record (the one `add` declares)
    /// and whether the paper is already declared in the library, by any of
    /// its records, for opening a detail view on it.
    pub fn selected_candidate(&self) -> Option<(CandidateWork, bool)> {
        let loaded = self.loaded()?;
        let row = loaded.rows().get(self.selected)?;
        Some((row.work.clone(), loaded.marks(row).in_library))
    }

    pub fn move_down(&mut self) {
        let len = self.row_count();
        if len > 0 {
            self.selected = (self.selected + 1) % len;
        }
    }

    pub fn move_up(&mut self) {
        let len = self.row_count();
        if len > 0 {
            self.selected = (self.selected + len - 1) % len;
        }
    }

    pub fn go_top(&mut self) {
        self.selected = 0;
    }

    pub fn go_bottom(&mut self) {
        self.selected = self.row_count().saturating_sub(1);
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

/// One provider's outcome, mirroring what `pax search` prints for it: the
/// count line (`retrieved N of M`), then any notes — more available, a
/// per-query ceiling, filters applied post-retrieval or not at all.
fn provider_lines<E: ToString>(
    provider: ProviderId,
    result: &Result<SearchPage, E>,
) -> Vec<String> {
    let page = match result {
        Ok(page) => page,
        Err(e) => return vec![format!("{provider}: failed — {}", e.to_string())],
    };
    let retrieved = page.retrieved();
    let Some(total) = page.total else {
        return vec![format!(
            "{provider}: retrieved {retrieved} (total not reported)"
        )];
    };
    let scanned = page.scanned;
    let mut count = format!("{provider}: retrieved {retrieved} of {total}");
    let mut notes = Vec::new();
    if page.is_complete() {
        let skipped = scanned.saturating_sub(retrieved as u64);
        if skipped > 0 && !page.has_post_retrieval_filter() {
            notes.push(format!(
                "{skipped} entries without a {provider} record skipped"
            ));
        }
    } else if let Some(max) = page.max_retrievable.filter(|_| at_ceiling(page)) {
        count.push_str(&format!(" — {provider} serves at most {max} per query"));
    } else {
        count.push_str(" — more available");
    }
    for filter in &page.filters {
        match filter.application {
            FilterApplication::PostRetrieval => notes.push(format!(
                "{} filter applied post-retrieval to the {scanned} records fetched; the total counts records before it",
                filter.kind
            )),
            FilterApplication::NotApplied => notes.push(format!(
                "{} filter not applied: {provider} has no such data, so these results ignore it",
                filter.kind
            )),
            _ => {}
        }
    }
    let unknown = page.language_unknown.len();
    if unknown > 0 {
        notes.push(format!(
            "{unknown} records have no language, so the language filter couldn't check them; kept and marked [language unknown]"
        ));
    }
    std::iter::once(count)
        .chain(
            notes
                .into_iter()
                .map(|note| format!("  {provider}: {note}")),
        )
        .collect()
}

/// Every provider's lines. A provider with no record of the seed or DOI,
/// while another one listed results, is a note rather than a failure, as
/// `pax refs` prints it.
fn summary_lines(loaded: &LoadedSearch) -> Vec<String> {
    let listed = loaded.results.values().any(Result::is_ok);
    PROVIDER_ORDER
        .iter()
        .filter_map(|p| {
            loaded.results.get(p).map(|result| match result {
                Err(ProviderFailure {
                    message,
                    no_record: true,
                }) if listed => vec![format!("{p}: {message}")],
                result => provider_lines(*p, result),
            })
        })
        .flatten()
        .collect()
}

pub fn loading_message(request: &SearchRequest) -> String {
    if !request.is_paged() {
        return "Looking up…".to_string();
    }
    let verb = match request.citations_label() {
        Some(label) => format!("Listing {label}"),
        None => "Searching".to_string(),
    };
    match request.options.limit {
        Limit::Count(n) => format!("{verb} (up to {n} results per provider)…"),
        _ => format!("{verb} (every result each provider serves; this can be slow)…"),
    }
}

pub fn draw(frame: &mut Frame, screen: &SearchScreen, editing: Option<InsertTarget>, area: Rect) {
    let header = header_text(screen, editing);
    let header_height = wrapped_height(&header, area.width).min(area.height / 2);
    let [header_area, list_area] =
        Layout::vertical([Constraint::Length(header_height), Constraint::Min(0)]).areas(area);

    frame.render_widget(
        Paragraph::new(header).wrap(Wrap { trim: false }),
        header_area,
    );

    match &screen.state {
        SearchState::Idle => {
            render_message(frame, list_area, "Type a query and press Enter to search.")
        }
        SearchState::Loading(request) => {
            render_message(frame, list_area, &loading_message(request))
        }
        SearchState::Loaded(loaded) => {
            if loaded.rows().is_empty() {
                render_message(frame, list_area, "No results.");
            } else {
                render_list(frame, loaded, screen.selected, list_area);
            }
        }
    }
}

fn header_text(screen: &SearchScreen, editing: Option<InsertTarget>) -> String {
    let field = |target, buffer: &str, committed: String| {
        if editing == Some(target) {
            format!("{buffer}▏")
        } else {
            committed
        }
    };
    let query = field(
        InsertTarget::SearchQuery,
        &screen.query_buffer,
        screen.query.clone(),
    );
    let filters = field(InsertTarget::SearchFilters, &screen.filter_buffer, {
        let set = format_filters(&screen.filters);
        if set.is_empty() {
            "none".to_string()
        } else {
            set
        }
    });
    let mut lines = vec![
        format!(
            "Search [{}] (Tab: cycle mode, Enter: search, /: edit, Esc: back): {query}",
            screen.kind.label()
        ),
        format!("Filters (F: edit; year:YYYY[-YYYY] type:TYPE lang:CODE): {filters}"),
    ];
    // A citation list isn't a typed query: say whose list is showing, under
    // the query line a new search would start from.
    if let Some(label) = screen.current_request().and_then(|r| r.citations_label()) {
        lines.push(format!("Showing {label}"));
    }
    if let Some(loaded) = screen.loaded() {
        lines.extend(summary_lines(loaded));
    }
    lines.join("\n")
}

fn render_message(frame: &mut Frame, area: Rect, message: &str) {
    let block = Block::default()
        .title(" lazypax — search ")
        .borders(Borders::ALL);
    frame.render_widget(Paragraph::new(message).block(block), area);
}

/// The list's title: how many papers, the merge counts (records before →
/// papers after, as `pax search`'s `merged:` line), and the limit.
fn list_title(loaded: &LoadedSearch) -> String {
    let merged = &loaded.merged;
    let what = if loaded.request.kind.is_citations() {
        loaded.request.kind.label()
    } else {
        "search"
    };
    let mut title = format!(
        " lazypax — {what} · {} papers · merged {} → {} ({} duplicates, {} unmatched)",
        merged.after,
        merged.before,
        merged.after,
        merged.duplicates(),
        merged.unmatched
    );
    if loaded.request.is_paged() {
        title.push_str(&format!(
            " · limit {} per provider",
            loaded.request.limit_label()
        ));
    }
    title.push(' ');
    title
}

fn render_list(frame: &mut Frame, loaded: &LoadedSearch, selected: usize, area: Rect) {
    let list_items: Vec<ListItem> = loaded
        .rows()
        .iter()
        .map(|row| ListItem::new(format_row(row, &loaded.marks(row))))
        .collect();
    let list = List::new(list_items)
        .block(
            Block::default()
                .title(list_title(loaded))
                .borders(Borders::ALL),
        )
        .highlight_style(SELECTED_ROW_STYLE);
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

/// One paper: the providers its records came from, the representative
/// record's title with the row's flags, then its details.
fn format_row(row: &MergedWork, marks: &Marks) -> String {
    let work = &row.work;
    let providers: Vec<String> = row.providers().iter().map(|p| p.to_string()).collect();
    let authors = if work.authors.is_empty() {
        "—".to_string()
    } else {
        work.authors.join(", ")
    };
    let year: String = work.publish_date.chars().take(4).collect();
    let venue = work.venue.as_deref().unwrap_or("no venue");
    let doi = work.doi.as_deref().unwrap_or("no doi");
    let pdf = if work.pdf_url.is_some() { "✓" } else { "✗" };
    let flag = |set: bool, text: &'static str| if set { text } else { "" };

    format!(
        "[{}] {}{}{}{}\n    {authors} · {year} · {venue} · DOI: {doi} · PDF: {pdf}",
        providers.join("+"),
        work.title,
        flag(marks.in_library, " [in library]"),
        flag(marks.excluded, " [excluded]"),
        flag(marks.language_unknown, " [language unknown]"),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use pax_core::{AppliedFilter, CandidateId, FilterKind, ProviderError};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    pub(crate) fn work(provider: ProviderId, native_id: &str, title: &str) -> CandidateWork {
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

    pub(crate) fn page(works: Vec<CandidateWork>, total: u64) -> SearchPage {
        SearchPage::new(works, Some(total), None)
    }

    fn loaded(results: SearchResults) -> SearchScreen {
        SearchScreen {
            state: SearchState::Loaded(Box::new(LoadedSearch::new(
                SearchRequest::new(SearchKind::Free, "q"),
                results,
                LibraryIndex::default(),
            ))),
            ..Default::default()
        }
    }

    pub(crate) fn render(screen: &SearchScreen) -> String {
        let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
        terminal
            .draw(|frame| draw(frame, screen, None, frame.area()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn rows_preserve_provider_order_and_skip_errors() {
        let mut results = SearchResults::new();
        results.insert(
            ProviderId::Crossref,
            Ok(page(vec![work(ProviderId::Crossref, "1", "B")], 1)),
        );
        results.insert(
            ProviderId::OpenAlex,
            Ok(page(vec![work(ProviderId::OpenAlex, "2", "A")], 1)),
        );
        results.insert(
            ProviderId::SemanticScholar,
            Err(ProviderError::NotFound.into()),
        );

        let screen = loaded(results);
        let rows = screen.loaded().unwrap().rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].work.title, "A"); // OpenAlex before Crossref, per PROVIDER_ORDER
        assert_eq!(rows[1].work.title, "B");
    }

    /// The same paper from OpenAlex and Crossref, by DOI.
    fn one_paper_from_two_providers() -> SearchResults {
        let with_doi = |provider, id| {
            let mut w = work(provider, id, "Actors");
            w.doi = Some("10.1/actors".to_string());
            w
        };
        let mut results = SearchResults::new();
        results.insert(
            ProviderId::OpenAlex,
            Ok(page(vec![with_doi(ProviderId::OpenAlex, "W1")], 1)),
        );
        results.insert(
            ProviderId::Crossref,
            Ok(page(
                vec![
                    with_doi(ProviderId::Crossref, "10.1/actors"),
                    work(ProviderId::Crossref, "2", "Other"),
                ],
                2,
            )),
        );
        results
    }

    #[test]
    fn records_of_one_paper_merge_into_one_row() {
        let screen = loaded(one_paper_from_two_providers());
        let search = screen.loaded().unwrap();
        assert_eq!(search.rows().len(), 2);
        assert_eq!(
            search.rows()[0].providers(),
            vec![ProviderId::OpenAlex, ProviderId::Crossref]
        );
        assert_eq!((search.merged.before, search.merged.after), (3, 2));

        let text = render(&screen);
        assert!(text.contains("[openalex+crossref] Actors"), "{text}");
        assert!(text.contains("2 papers · merged 3 → 2 (1 duplicates, 0 unmatched)"));
    }

    #[test]
    fn a_row_is_flagged_excluded_by_any_of_its_records() {
        let mut exclusions = pax_core::Exclusions::default();
        exclusions.upsert(pax_core::Exclusion::new(
            "10.1/actors".to_string(),
            "Actors".to_string(),
            "off topic".to_string(),
            pax_core::ExclusionStage::Screening,
        ));
        let index = LibraryIndex::new(&pax_core::Library::new(Vec::new()), &exclusions);
        let screen = SearchScreen {
            state: SearchState::Loaded(Box::new(LoadedSearch::new(
                SearchRequest::new(SearchKind::Free, "q"),
                one_paper_from_two_providers(),
                index,
            ))),
            ..Default::default()
        };
        let search = screen.loaded().unwrap();
        assert!(search.marks(&search.rows()[0]).excluded);
        assert!(!search.marks(&search.rows()[1]).excluded);
        assert!(render(&screen).contains("Actors [excluded]"));
    }

    #[test]
    fn language_unknown_marks_a_row_only_when_every_record_is() {
        let results = one_paper_from_two_providers();
        let crossref_actors = results[&ProviderId::Crossref].as_ref().unwrap().works[0]
            .id
            .clone();
        let mut results = results;
        let crossref = results.remove(&ProviderId::Crossref).unwrap().unwrap();
        results.insert(
            ProviderId::Crossref,
            Ok(crossref.with_language_unknown(vec![crossref_actors])),
        );
        let screen = loaded(results);
        let search = screen.loaded().unwrap();
        // OpenAlex's record of the same paper has a language.
        assert!(!search.marks(&search.rows()[0]).language_unknown);
    }

    #[test]
    fn a_complete_page_shows_retrieved_of_total() {
        let lines = provider_lines::<ProviderError>(
            ProviderId::ArXiv,
            &Ok(page(vec![work(ProviderId::ArXiv, "1", "A")], 1)),
        );
        assert_eq!(lines, vec!["arxiv: retrieved 1 of 1"]);
    }

    #[test]
    fn a_partial_page_says_more_is_available() {
        let lines = provider_lines::<ProviderError>(
            ProviderId::OpenAlex,
            &Ok(page(vec![work(ProviderId::OpenAlex, "1", "A")], 900)),
        );
        assert_eq!(lines, vec!["openalex: retrieved 1 of 900 — more available"]);
    }

    #[test]
    fn a_page_at_the_providers_ceiling_says_so_instead() {
        let works = (0..3)
            .map(|n| work(ProviderId::SemanticScholar, &n.to_string(), "T"))
            .collect();
        let capped = SearchPage::new(works, Some(5000), Some(3));
        let lines = provider_lines::<ProviderError>(ProviderId::SemanticScholar, &Ok(capped));
        assert_eq!(
            lines,
            vec![
                "semanticscholar: retrieved 3 of 5000 — semanticscholar serves at most 3 per query"
            ]
        );
    }

    #[test]
    fn an_unreported_total_says_so() {
        let lines = provider_lines::<ProviderError>(
            ProviderId::Crossref,
            &Ok(SearchPage::new(Vec::new(), None, None)),
        );
        assert_eq!(lines, vec!["crossref: retrieved 0 (total not reported)"]);
    }

    #[test]
    fn a_failed_provider_shows_its_error() {
        let lines = provider_lines(ProviderId::Crossref, &Err(ProviderError::NotFound));
        assert_eq!(lines, vec!["crossref: failed — not found"]);
    }

    #[test]
    fn filter_notes_follow_the_count_line() {
        let page = page(Vec::new(), 10).with_scanned(10).with_filters(vec![
            AppliedFilter::new(FilterKind::Year, FilterApplication::Server),
            AppliedFilter::new(FilterKind::Language, FilterApplication::PostRetrieval),
            AppliedFilter::new(FilterKind::DocType, FilterApplication::NotApplied),
        ]);
        let lines = provider_lines::<ProviderError>(ProviderId::Crossref, &Ok(page));
        assert_eq!(lines.len(), 3);
        assert!(lines[1].contains("language filter applied post-retrieval to the 10 records"));
        assert!(lines[2].contains("type filter not applied: crossref has no such data"));
    }

    #[test]
    fn has_more_only_while_some_provider_can_serve_more() {
        let mut results = SearchResults::new();
        results.insert(ProviderId::OpenAlex, Ok(page(Vec::new(), 0)));
        assert!(!loaded(results.clone()).loaded().unwrap().has_more());

        results.insert(
            ProviderId::Crossref,
            Ok(page(vec![work(ProviderId::Crossref, "1", "A")], 50)),
        );
        assert!(loaded(results).loaded().unwrap().has_more());
    }

    #[test]
    fn a_doi_lookup_never_has_more() {
        let mut results = SearchResults::new();
        results.insert(
            ProviderId::Crossref,
            Ok(page(vec![work(ProviderId::Crossref, "1", "A")], 50)),
        );
        let doi = LoadedSearch::new(
            SearchRequest::new(SearchKind::Doi, "10.1/x"),
            results,
            LibraryIndex::default(),
        );
        assert!(!doi.has_more());
    }

    #[test]
    fn movement_wraps_across_results() {
        let mut results = SearchResults::new();
        results.insert(
            ProviderId::OpenAlex,
            Ok(page(
                vec![
                    work(ProviderId::OpenAlex, "1", "A"),
                    work(ProviderId::OpenAlex, "2", "B"),
                ],
                2,
            )),
        );
        let mut screen = loaded(results);
        screen.selected = 1;
        screen.move_down();
        assert_eq!(screen.selected, 0);
        screen.move_up();
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn loading_more_of_the_same_search_keeps_the_selection() {
        let results_of = |n: usize| {
            let mut results = SearchResults::new();
            let works = (0..n)
                .map(|i| work(ProviderId::OpenAlex, &i.to_string(), &format!("T{i}")))
                .collect();
            results.insert(ProviderId::OpenAlex, Ok(page(works, 10)));
            results
        };
        let mut screen = loaded(results_of(2));
        screen.selected = 1;
        let request = SearchRequest::new(SearchKind::Free, "q").more().unwrap();
        screen.show_results(LoadedSearch::new(
            request,
            results_of(4),
            LibraryIndex::default(),
        ));
        assert_eq!(screen.selected, 1);

        screen.show_results(LoadedSearch::new(
            SearchRequest::new(SearchKind::Free, "other"),
            results_of(4),
            LibraryIndex::default(),
        ));
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn selected_candidate_reports_in_library_by_identity() {
        let mut w = work(ProviderId::OpenAlex, "1", "A");
        w.doi = Some("https://doi.org/10.1145/foo".to_string());
        let mut results = SearchResults::new();
        results.insert(ProviderId::OpenAlex, Ok(page(vec![w], 1)));
        let paper = pax_core::Paper {
            identity: pax_core::Identity {
                doi: Some("10.1145/foo".to_string()),
                title: "A".to_string(),
                ..Default::default()
            },
            artifact: pax_core::Artifact::default(),
            local: pax_core::Local {
                citation_key: "a2020".to_string(),
                ..Default::default()
            },
        };
        let index = LibraryIndex::new(
            &pax_core::Library::new(vec![paper]),
            &pax_core::Exclusions::default(),
        );
        let screen = SearchScreen {
            state: SearchState::Loaded(Box::new(LoadedSearch::new(
                SearchRequest::new(SearchKind::Free, "q"),
                results,
                index,
            ))),
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

    #[test]
    fn the_rendered_header_shows_each_providers_totals() {
        let mut results = SearchResults::new();
        results.insert(
            ProviderId::OpenAlex,
            Ok(page(vec![work(ProviderId::OpenAlex, "1", "Actors")], 120)),
        );
        results.insert(
            ProviderId::Crossref,
            Err(ProviderError::Request("429 Too Many Requests".to_string()).into()),
        );
        let text = render(&loaded(results));
        assert!(text.contains("openalex: retrieved 1 of 120 — more available"));
        assert!(text.contains("crossref: failed — request failed: 429 Too Many Requests"));
        assert!(text.contains("Actors"));
        assert!(text.contains(
            "1 papers · merged 1 → 1 (0 duplicates, 0 unmatched) · limit 50 per provider"
        ));
    }

    #[test]
    fn loading_shows_the_limit_being_retrieved() {
        let more = SearchRequest::new(SearchKind::Free, "q").more().unwrap();
        assert_eq!(
            loading_message(&more),
            "Searching (up to 100 results per provider)…"
        );
        let all = more.all().unwrap();
        assert!(loading_message(&all).contains("every result each provider serves"));
    }

    #[test]
    fn loading_a_citation_list_names_it() {
        let refs = SearchRequest::citations(SearchKind::References, "turing1936", "OCN");
        assert_eq!(
            loading_message(&refs),
            "Listing references of \"OCN\" (turing1936) (up to 50 results per provider)…"
        );
    }

    #[test]
    fn a_provider_without_the_seed_is_a_note_only_while_another_listed() {
        let no_record = || {
            Err(ProviderFailure {
                message: "semanticscholar has no record of the seed".to_string(),
                no_record: true,
            })
        };
        let mut results = SearchResults::new();
        results.insert(ProviderId::SemanticScholar, no_record());
        let alone = render(&loaded(results.clone()));
        assert!(alone.contains("semanticscholar: failed — semanticscholar has no record"));

        results.insert(
            ProviderId::OpenAlex,
            Ok(page(vec![work(ProviderId::OpenAlex, "1", "A")], 1)),
        );
        let text = render(&loaded(results));
        assert!(text.contains("semanticscholar: semanticscholar has no record of the seed"));
        assert!(!text.contains("failed"));
    }

    #[test]
    fn a_filter_change_re_runs_the_citation_list_showing_from_the_default_limit() {
        let refs = SearchRequest::citations(SearchKind::CitedBy, "openalex:W1", "T")
            .all()
            .unwrap();
        let screen = SearchScreen {
            filters: parse_filters("year:2010").unwrap(),
            state: SearchState::Loading(refs),
            ..Default::default()
        };
        let request = screen.refiltered_request();
        assert_eq!(request.kind, SearchKind::CitedBy);
        assert_eq!(request.query, "openalex:W1");
        assert_eq!(request.options.limit, Limit::Count(50));
        assert_eq!(request.options.filters, screen.filters);
        assert!(screen.is_showing(&request));
    }
}
