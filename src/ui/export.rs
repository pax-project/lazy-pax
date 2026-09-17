use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

#[derive(Default)]
pub struct ExportScreen {
    /// The rendered BibTeX, computed once when the screen is entered.
    pub bibtex: String,
    /// The committed file path to write to on save.
    pub path: String,
    /// Live edit buffer while `InsertTarget::ExportPath` is active; only
    /// copied into `path` on submit.
    pub buffer: String,
}

pub fn draw(frame: &mut Frame, screen: &ExportScreen, editing: bool, area: Rect) {
    let [preview_area, path_area] = Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).areas(area);

    let preview_text = if screen.bibtex.is_empty() {
        "Library is empty.".to_string()
    } else {
        screen.bibtex.clone()
    };
    let preview_block = Block::default().title(" lazypax — export (BibTeX) ").borders(Borders::ALL);
    frame.render_widget(
        Paragraph::new(preview_text).block(preview_block).wrap(Wrap { trim: false }),
        preview_area,
    );

    let path_text = if editing {
        format!("{}▏", screen.buffer)
    } else if screen.path.is_empty() {
        "(no path set)".to_string()
    } else {
        screen.path.clone()
    };
    let path_block = Block::default().title(" path ").borders(Borders::ALL);
    frame.render_widget(Paragraph::new(path_text).block(path_block), path_area);
}
