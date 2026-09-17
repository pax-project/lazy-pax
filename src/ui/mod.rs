pub mod detail;
pub mod library;
pub mod search;

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
        Screen::Search => {
            let editing = matches!(app.mode, Mode::Insert(InsertTarget::SearchQuery));
            search::draw(frame, &app.search, editing, content);
        }
        Screen::Detail => detail::draw(frame, &app.detail, content),
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
    match app.screen {
        Screen::Library => match &app.mode {
            Mode::Insert(InsertTarget::LibraryFilter) => {
                format!("Filter: {}▏  (Enter: apply, Esc: cancel)", app.library.filter_buffer)
            }
            _ if !app.library.query.is_empty() => {
                format!("Filter: {}  (/: edit, Esc: clear, S: search, q: quit)", app.library.query)
            }
            _ => "/: filter  S: search  q: quit".to_string(),
        },
        Screen::Search => "Enter/l: view  Esc: back  q: quit".to_string(),
        Screen::Detail => "Esc: back  q: quit".to_string(),
    }
}
