use pax_core::Paper;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table, TableState};
use ratatui::Frame;

pub enum LibraryState {
    Loading,
    NotInitialized,
    Loaded(Vec<Paper>),
}

pub struct LibraryScreen {
    pub state: LibraryState,
    pub selected: usize,
}

impl Default for LibraryScreen {
    fn default() -> Self {
        Self {
            state: LibraryState::Loading,
            selected: 0,
        }
    }
}

impl LibraryScreen {
    fn papers(&self) -> Option<&[Paper]> {
        match &self.state {
            LibraryState::Loaded(papers) => Some(papers),
            _ => None,
        }
    }

    pub fn move_down(&mut self) {
        if let Some(papers) = self.papers().filter(|p| !p.is_empty()) {
            self.selected = (self.selected + 1) % papers.len();
        }
    }

    pub fn move_up(&mut self) {
        if let Some(papers) = self.papers().filter(|p| !p.is_empty()) {
            self.selected = (self.selected + papers.len() - 1) % papers.len();
        }
    }

    pub fn go_top(&mut self) {
        self.selected = 0;
    }

    pub fn go_bottom(&mut self) {
        if let Some(papers) = self.papers().filter(|p| !p.is_empty()) {
            self.selected = papers.len() - 1;
        }
    }
}

pub fn draw(frame: &mut Frame, screen: &LibraryScreen, area: Rect) {
    match &screen.state {
        LibraryState::Loading => render_message(frame, area, "Loading library…"),
        LibraryState::NotInitialized => render_message(
            frame,
            area,
            "No pax library found here.\n\nRun `pax init` in this directory to create one.",
        ),
        LibraryState::Loaded(papers) if papers.is_empty() => {
            render_message(frame, area, "Library is empty.\n\nSearch for papers to add them.")
        }
        LibraryState::Loaded(papers) => render_table(frame, papers, screen.selected, area),
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
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pax_core::{Artifact, Identity, Local};

    fn paper(key: &str) -> Paper {
        Paper {
            identity: Identity {
                title: key.to_string(),
                ..Default::default()
            },
            artifact: Artifact::default(),
            local: Local {
                citation_key: key.to_string(),
                ..Default::default()
            },
        }
    }

    fn loaded(keys: &[&str]) -> LibraryScreen {
        LibraryScreen {
            state: LibraryState::Loaded(keys.iter().map(|k| paper(k)).collect()),
            selected: 0,
        }
    }

    #[test]
    fn move_down_wraps_around() {
        let mut screen = loaded(&["a", "b", "c"]);
        screen.selected = 2;
        screen.move_down();
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn move_up_wraps_around() {
        let mut screen = loaded(&["a", "b", "c"]);
        screen.selected = 0;
        screen.move_up();
        assert_eq!(screen.selected, 2);
    }

    #[test]
    fn go_top_and_bottom() {
        let mut screen = loaded(&["a", "b", "c"]);
        screen.selected = 1;
        screen.go_bottom();
        assert_eq!(screen.selected, 2);
        screen.go_top();
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn movement_on_empty_or_loading_library_is_a_no_op() {
        let mut screen = LibraryScreen::default();
        screen.move_down();
        screen.move_up();
        screen.go_bottom();
        assert_eq!(screen.selected, 0);

        let mut empty = loaded(&[]);
        empty.move_down();
        assert_eq!(empty.selected, 0);
    }
}
