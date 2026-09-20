pub mod detail;
pub mod edit;
pub mod export;
pub mod library;
pub mod reports;
pub mod search;

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, InsertTarget, Mode, Screen};
use crate::status::StatusKind;
use crate::ui::detail::DetailSubject;
use crate::ui::library::LibraryState;

/// The selected-row style shared by every list/table in the app. Explicit
/// fg/bg rather than `Modifier::REVERSED`: reverse video swaps whatever
/// colors happen to already be set (including a row's own item color, e.g.
/// red/green status text), and the *result* depends on the terminal's own
/// default palette — on some color schemes that leaves the selected row
/// nearly unreadable. Pinning both colors makes the highlight legible
/// regardless of terminal theme or the row's own coloring.
pub const SELECTED_ROW_STYLE: Style = Style::new().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD);

/// Rows the status/hint line is allowed to grow to. A one-line cap silently
/// clipped long text off the edge of the terminal (e.g. a failed fetch's URL
/// running past the screen width) — exactly the kind of swallowed error the
/// DoD's "no silent failure" requirement rules out. Still capped rather than
/// unbounded, so one runaway message can't push the whole screen's content
/// off-screen.
const STATUS_MAX_HEIGHT: u16 = 4;

pub fn draw(frame: &mut Frame, app: &App) {
    let full_area = frame.area();
    let (status_text, status_style) = status_content(app);
    let status_height = wrapped_height(&status_text, full_area.width).min(STATUS_MAX_HEIGHT);
    let [content, status] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(status_height)]).areas(full_area);

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
        Screen::SyncReport => {
            let empty = Vec::new();
            let syncs = app.sync_report.as_ref().unwrap_or(&empty);
            reports::draw_sync(frame, syncs, app.sync_selected, content);
        }
        Screen::CheckReport => {
            let empty = Vec::new();
            let checks = app.check_report.as_ref().unwrap_or(&empty);
            reports::draw_check(frame, checks, app.check_selected, content);
        }
        Screen::Export => {
            let editing = matches!(app.mode, Mode::Insert(InsertTarget::ExportPath));
            export::draw(frame, &app.export, editing, content);
        }
    }

    frame.render_widget(
        Paragraph::new(status_text).style(status_style).wrap(Wrap { trim: false }),
        status,
    );
}

/// The status line's text and style — a confirmation prompt, a job outcome,
/// or (absent either) the screen's own keybinding hints. Split out from
/// rendering so `draw` can size the status area to fit the text *before*
/// laying out the rest of the frame around it.
fn status_content(app: &App) -> (String, Style) {
    if let Some(prompt) = &app.confirm {
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
    }
}

/// How many rows `text` needs to word-wrap into within `width` columns —
/// mirrors `Paragraph`'s own `Wrap { trim: false }` behavior closely enough
/// to size the area ahead of render (an exact match isn't required: this
/// only decides *how much room* the wrapped paragraph gets, not how it
/// wraps). At least 1, even for empty text, since the status line is always
/// shown.
fn wrapped_height(text: &str, width: u16) -> u16 {
    if width == 0 {
        return 1;
    }
    text.lines()
        .map(|line| (line.chars().count() as u16).div_ceil(width).max(1))
        .sum::<u16>()
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_needs_a_single_row() {
        assert_eq!(wrapped_height("q: quit", 80), 1);
    }

    #[test]
    fn text_longer_than_the_width_wraps_into_more_rows() {
        // A failed fetch's error (e.g. a long dl.acm.org PDF URL) is exactly
        // the case this exists for: previously clipped silently at one row.
        let text = "backus1978: ERROR — fetch failed: error: unable to download \
                     'https://dl.acm.org/doi/pdf/10.1145/359576.359579'";
        assert_eq!(wrapped_height(text, 40), 3);
    }

    #[test]
    fn empty_text_still_reserves_one_row() {
        assert_eq!(wrapped_height("", 80), 1);
    }

    #[test]
    fn zero_width_does_not_divide_by_zero() {
        assert_eq!(wrapped_height("anything", 0), 1);
    }
}

fn hint_text(app: &App) -> String {
    match app.screen {
        Screen::Library if matches!(app.library.state, LibraryState::NotInitialized) => {
            "i: initialize  q: quit".to_string()
        }
        Screen::Library => match &app.mode {
            Mode::Insert(InsertTarget::LibraryFilter) => {
                format!("Filter: {}▏  (Enter: apply, Esc: cancel)", app.library.filter_buffer)
            }
            _ if !app.library.query.is_empty() => {
                format!(
                    "Filter: {}  (Enter/l, f: fetch, o: open, e: edit, d: remove, s: sync, c: check, E: export, /: edit filter, Esc: clear, S: search, q: quit)",
                    app.library.query
                )
            }
            _ => {
                "Enter/l: view  f: fetch  o: open  e: edit  d: remove  s: sync  c: check  E: export  /: filter  S: search  q: quit"
                    .to_string()
            }
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
            Mode::Normal => {
                "j/k: field  i/Enter: edit  a: add tag  x: remove tag  u: upload pdf  w: save  Esc: back".to_string()
            }
        },
        Screen::SyncReport | Screen::CheckReport => "j/k: scroll  Esc: back  q: quit".to_string(),
        Screen::Export => match &app.mode {
            Mode::Insert(_) => "Enter: apply  Esc: cancel".to_string(),
            Mode::Normal => "i/p: set path  w: write  Esc: back  q: quit".to_string(),
        },
    }
}
