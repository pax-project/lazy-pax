use std::collections::HashSet;

use pax_core::{ListFilter, Paper};
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::ui::SELECTED_ROW_STYLE;

pub enum LibraryState {
    Loading,
    NotInitialized,
    Loaded(Vec<Paper>),
}

pub struct LibraryScreen {
    pub state: LibraryState,
    pub selected: usize,
    /// The committed filter text (empty = no filter applied).
    pub query: String,
    /// Live edit buffer while `Mode::Insert(InsertTarget::LibraryFilter)` is
    /// active; only copied into `query` on submit.
    pub filter_buffer: String,
}

impl Default for LibraryScreen {
    fn default() -> Self {
        Self {
            state: LibraryState::Loading,
            selected: 0,
            query: String::new(),
            filter_buffer: String::new(),
        }
    }
}

impl LibraryScreen {
    fn all_papers(&self) -> Option<&[Paper]> {
        match &self.state {
            LibraryState::Loaded(papers) => Some(papers),
            _ => None,
        }
    }

    /// The papers currently on screen: every declared paper, narrowed by
    /// `query` if one is set. `None` while there's nothing loaded yet.
    pub fn visible_papers(&self) -> Option<Vec<Paper>> {
        self.all_papers().map(|papers| filter_by_query(papers, &self.query))
    }

    /// Every declared paper, ignoring any applied filter — for export,
    /// which per the DoD covers the whole library, not the filtered view.
    pub fn declared_papers(&self) -> &[Paper] {
        self.all_papers().unwrap_or(&[])
    }

    pub fn move_down(&mut self) {
        if let Some(papers) = self.visible_papers().filter(|p| !p.is_empty()) {
            self.selected = (self.selected + 1) % papers.len();
        }
    }

    pub fn move_up(&mut self) {
        if let Some(papers) = self.visible_papers().filter(|p| !p.is_empty()) {
            self.selected = (self.selected + papers.len() - 1) % papers.len();
        }
    }

    pub fn go_top(&mut self) {
        self.selected = 0;
    }

    pub fn go_bottom(&mut self) {
        if let Some(papers) = self.visible_papers().filter(|p| !p.is_empty()) {
            self.selected = papers.len() - 1;
        }
    }

    /// The currently-highlighted paper, for opening a detail view on it.
    pub fn selected_paper(&self) -> Option<Paper> {
        self.visible_papers()?.get(self.selected).cloned()
    }

    pub fn clear_filter(&mut self) {
        self.query.clear();
        self.selected = 0;
    }

    pub fn begin_filter_edit(&mut self) {
        self.filter_buffer = self.query.clone();
    }

    pub fn submit_filter_edit(&mut self) {
        self.query = self.filter_buffer.clone();
        self.selected = 0;
    }
}

/// Matches `query` against each of `filter_papers`'s independent criteria
/// (author substring, exact tag, exact year if `query` parses as one) and
/// unions the results, rather than ANDing all three against the same
/// string (which would only ever match a paper whose author, tag, *and*
/// year were all literally the same text). Reuses `pax_core`'s own
/// matching semantics for each field instead of hand-rolling substring/tag
/// comparisons here.
fn filter_by_query(papers: &[Paper], query: &str) -> Vec<Paper> {
    let query = query.trim();
    if query.is_empty() {
        return papers.to_vec();
    }

    let mut matching_keys: HashSet<String> = HashSet::new();
    let by_author = pax_core::filter_papers(
        papers,
        &ListFilter {
            author: Some(query.to_string()),
            ..Default::default()
        },
    );
    matching_keys.extend(by_author.into_iter().map(|p| p.local.citation_key));

    let by_tag = pax_core::filter_papers(
        papers,
        &ListFilter {
            tag: Some(query.to_string()),
            ..Default::default()
        },
    );
    matching_keys.extend(by_tag.into_iter().map(|p| p.local.citation_key));

    if let Ok(year) = query.parse::<i32>() {
        let by_year = pax_core::filter_papers(
            papers,
            &ListFilter {
                year: Some(year),
                ..Default::default()
            },
        );
        matching_keys.extend(by_year.into_iter().map(|p| p.local.citation_key));
    }

    papers
        .iter()
        .filter(|p| matching_keys.contains(&p.local.citation_key))
        .cloned()
        .collect()
}

pub fn draw(frame: &mut Frame, screen: &LibraryScreen, area: Rect) {
    match &screen.state {
        LibraryState::Loading => render_message(frame, area, "Loading library…"),
        LibraryState::NotInitialized => render_message(
            frame,
            area,
            "No pax library found here.\n\nPress i to initialize one.",
        ),
        LibraryState::Loaded(papers) if papers.is_empty() => {
            render_message(frame, area, "Library is empty.\n\nSearch for papers to add them.")
        }
        LibraryState::Loaded(papers) => {
            let visible = filter_by_query(papers, &screen.query);
            if visible.is_empty() {
                render_message(frame, area, "No papers match the current filter.")
            } else {
                render_table(frame, &visible, screen.selected, area)
            }
        }
    }
}

fn render_message(frame: &mut Frame, area: Rect, message: &str) {
    let block = Block::default().title(" lazypax — library ").borders(Borders::ALL);
    let paragraph = Paragraph::new(message).block(block).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

fn render_table(frame: &mut Frame, papers: &[Paper], selected: usize, area: Rect) {
    let header = Row::new(["", "Key", "Title", "Authors", "Year"]).style(Style::default().add_modifier(Modifier::BOLD));
    let rows = papers.iter().map(|paper| {
        let fetched = if paper.artifact.hash.is_some() { "✓" } else { "✗" };
        let year = paper
            .identity
            .year
            .map(|y| y.to_string())
            .unwrap_or_default();
        Row::new([
            fetched.to_string(),
            paper.local.citation_key.clone(),
            paper.identity.title.clone(),
            paper.identity.authors.join(", "),
            year,
        ])
    });
    let widths = [
        Constraint::Length(1),
        Constraint::Length(16),
        Constraint::Percentage(45),
        Constraint::Percentage(30),
        Constraint::Length(6),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().title(" lazypax — library ").borders(Borders::ALL))
        .row_highlight_style(SELECTED_ROW_STYLE);
    let mut state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pax_core::{Artifact, Identity, Local};

    fn paper(key: &str, author: &str, year: i32, tags: &[&str]) -> Paper {
        Paper {
            identity: Identity {
                title: key.to_string(),
                authors: vec![author.to_string()],
                year: Some(year),
                ..Default::default()
            },
            artifact: Artifact::default(),
            local: Local {
                citation_key: key.to_string(),
                tags: tags.iter().map(|t| t.to_string()).collect(),
                ..Default::default()
            },
        }
    }

    fn fixture() -> Vec<Paper> {
        vec![
            paper("turing1936", "Alan Turing", 1936, &["computability", "logic"]),
            paper("hewitt1973", "Carl Hewitt", 1973, &["concurrency"]),
        ]
    }

    fn loaded(papers: Vec<Paper>) -> LibraryScreen {
        LibraryScreen {
            state: LibraryState::Loaded(papers),
            selected: 0,
            query: String::new(),
            filter_buffer: String::new(),
        }
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(filter_by_query(&fixture(), "").len(), 2);
        assert_eq!(filter_by_query(&fixture(), "   ").len(), 2);
    }

    #[test]
    fn matches_author_substring_case_insensitively() {
        let result = filter_by_query(&fixture(), "hewitt");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].local.citation_key, "hewitt1973");
    }

    #[test]
    fn matches_exact_tag() {
        let result = filter_by_query(&fixture(), "concurrency");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].local.citation_key, "hewitt1973");
    }

    #[test]
    fn matches_exact_year() {
        let result = filter_by_query(&fixture(), "1936");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].local.citation_key, "turing1936");
    }

    #[test]
    fn no_match_returns_empty() {
        assert!(filter_by_query(&fixture(), "nonexistent").is_empty());
    }

    #[test]
    fn filter_edit_round_trips_through_begin_and_submit() {
        let mut screen = loaded(fixture());
        screen.query = "turing".to_string();
        screen.begin_filter_edit();
        assert_eq!(screen.filter_buffer, "turing");
        screen.filter_buffer = "hewitt".to_string();
        screen.submit_filter_edit();
        assert_eq!(screen.query, "hewitt");
        assert_eq!(screen.visible_papers().unwrap().len(), 1);
    }

    #[test]
    fn clear_filter_resets_query_and_selection() {
        let mut screen = loaded(fixture());
        screen.query = "hewitt".to_string();
        screen.selected = 3;
        screen.clear_filter();
        assert_eq!(screen.query, "");
        assert_eq!(screen.selected, 0);
        assert_eq!(screen.visible_papers().unwrap().len(), 2);
    }

    #[test]
    fn movement_operates_on_filtered_view_not_full_library() {
        let mut screen = loaded(fixture());
        screen.query = "hewitt".to_string();
        screen.move_down(); // only one match -> wraps to itself
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn selected_paper_reflects_the_filtered_view() {
        let mut screen = loaded(fixture());
        screen.selected = 1;
        assert_eq!(screen.selected_paper().unwrap().local.citation_key, "hewitt1973");

        screen.query = "turing".to_string();
        screen.selected = 0;
        assert_eq!(screen.selected_paper().unwrap().local.citation_key, "turing1936");
    }

    #[test]
    fn selected_paper_is_none_before_the_library_loads() {
        assert!(LibraryScreen::default().selected_paper().is_none());
    }

    #[test]
    fn declared_papers_ignores_the_filter() {
        let mut screen = loaded(fixture());
        screen.query = "hewitt".to_string();
        assert_eq!(screen.visible_papers().unwrap().len(), 1);
        assert_eq!(screen.declared_papers().len(), 2);
    }

    #[test]
    fn declared_papers_is_empty_before_the_library_loads() {
        assert!(LibraryScreen::default().declared_papers().is_empty());
    }
}
