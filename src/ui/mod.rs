pub mod library;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, Screen};

pub fn draw(frame: &mut Frame, app: &App) {
    let [content, status] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());

    match app.screen {
        Screen::Library => library::draw(frame, &app.library, content),
    }

    draw_status_bar(frame, app, status);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let text = app.status.message.as_deref().unwrap_or("q: quit");
    let style = if app.status.message.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}
