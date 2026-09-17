pub mod detail;
pub mod edit;
pub mod library;
pub mod search;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, InsertTarget, Mode, Screen};
use crate::status::StatusKind;
use crate::ui::detail::DetailSubject;

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
        Screen::Edit => {
            let editing_target = match app.mode {
                Mode::Insert(target) => Some(target),
                Mode::Normal => None,
            };
            edit::draw(frame, &app.edit, editing_target, content);
        }
    }

    draw_status_bar(frame, app, status);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let (text, style) = if let Some(prompt) = &app.confirm {
        (
            format!("{}  (y/n)", prompt.message),
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        )
    } else if let Some((kind, msg)) = &app.status.message {
        let color = match kind {
            StatusKind::Error => Color::Red,
            StatusKind::Success => Color::Green,
            StatusKind::Pending => Color::Yellow,
        };
        (msg.clone(), Style::default().fg(color))
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
                format!(
                    "Filter: {}  (Enter/l, f: fetch, o: open, e: edit, d: remove, /: edit filter, Esc: clear, S: search, q: quit)",
                    app.library.query
                )
            }
            _ => "Enter/l: view  f: fetch  o: open  e: edit  d: remove  /: filter  S: search  q: quit".to_string(),
        },
        Screen::Search => "Enter/l: view  Esc: back  q: quit".to_string(),
        Screen::Detail => match &app.detail.subject {
            Some(DetailSubject::Candidate { in_library: true, .. }) => {
                "a: add anyway (already in library)  Esc: back  q: quit".to_string()
            }
            Some(DetailSubject::Candidate { in_library: false, .. }) => "a: add  Esc: back  q: quit".to_string(),
            Some(DetailSubject::Declared(_)) => {
                "f: fetch  o: open  e: edit  d: remove  Esc: back  q: quit".to_string()
            }
            None => "Esc: back  q: quit".to_string(),
        },
        Screen::Edit => match &app.mode {
            Mode::Insert(_) => "Enter: apply  Esc: cancel".to_string(),
            Mode::Normal => "j/k: field  i/Enter: edit  a: add tag  x: remove tag  w: save  Esc: back".to_string(),
        },
    }
}
