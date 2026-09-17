pub mod library;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, InsertTarget, Mode, Screen};

pub fn draw(frame: &mut Frame, app: &App) {
    let [content, status] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());

    match app.screen {
        Screen::Library => library::draw(frame, &app.library, content),
    }

    draw_status_bar(frame, app, status);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let (text, style) = if let Some(msg) = &app.status.message {
        (msg.clone(), Style::default().fg(Color::Red))
    } else {
        (hint_text(app), Style::default())
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

fn hint_text(app: &App) -> String {
    match (&app.screen, &app.mode) {
        (Screen::Library, Mode::Insert(InsertTarget::LibraryFilter)) => {
            format!("Filter: {}▏  (Enter: apply, Esc: cancel)", app.library.filter_buffer)
        }
        (Screen::Library, Mode::Normal) if !app.library.query.is_empty() => {
            format!("Filter: {}  (/: edit, Esc: clear, q: quit)", app.library.query)
        }
        (Screen::Library, Mode::Normal) => "/: filter  q: quit".to_string(),
    }
}
