use ratatui::layout::{Alignment, Rect};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, Screen};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    match app.screen {
        Screen::Library => draw_library(frame, area),
    }
}

fn draw_library(frame: &mut Frame, area: Rect) {
    let block = Block::default().title(" lazypax ").borders(Borders::ALL);
    let paragraph = Paragraph::new("Library is empty.\n\nPress q to quit.")
        .block(block)
        .alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}
