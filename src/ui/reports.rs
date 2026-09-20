use pax_core::{CheckReport, CheckStatus, FetchOutcome, SyncReport};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::ui::SELECTED_ROW_STYLE;

/// Shared list rendering for `pax sync`/`pax check`'s per-paper results —
/// both are just "a citation key plus a one-line, colored outcome", so one
/// render function serves either report type via `to_line`.
fn draw_report<T>(
    frame: &mut Frame,
    title: &str,
    reports: &[T],
    selected: usize,
    area: Rect,
    to_line: impl Fn(&T) -> (String, Color),
) {
    if reports.is_empty() {
        let block = Block::default().title(title.to_string()).borders(Borders::ALL);
        frame.render_widget(
            ratatui::widgets::Paragraph::new("Library is empty.").block(block),
            area,
        );
        return;
    }
    let items: Vec<ListItem> = reports
        .iter()
        .map(|r| {
            let (text, color) = to_line(r);
            ListItem::new(text).style(Style::default().fg(color))
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().title(title.to_string()).borders(Borders::ALL))
        .highlight_style(SELECTED_ROW_STYLE);
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

pub fn draw_sync(frame: &mut Frame, reports: &[SyncReport], selected: usize, area: Rect) {
    draw_report(frame, " lazypax — sync ", reports, selected, area, sync_line);
}

pub fn draw_check(frame: &mut Frame, reports: &[CheckReport], selected: usize, area: Rect) {
    draw_report(frame, " lazypax — check ", reports, selected, area, check_line);
}

/// Mirrors the `pax` CLI's own `sink.rs::fetched()` wording, so a sync
/// result reads the same here as after a plain `pax fetch`/`pax sync`.
fn sync_line(report: &SyncReport) -> (String, Color) {
    match &report.result {
        Ok(FetchOutcome::Fetched { hash }) => {
            (format!("{}: fetched (hash {hash})", report.citation_key), Color::Green)
        }
        Ok(FetchOutcome::AlreadyFetched { hash }) => (
            format!("{}: already fetched (hash {hash})", report.citation_key),
            Color::Green,
        ),
        Err(e) => (format!("{}: ERROR — {e}", report.citation_key), Color::Red),
    }
}

/// Mirrors the `pax` CLI's own `sink.rs::checked()` per-line wording.
fn check_line(report: &CheckReport) -> (String, Color) {
    match &report.status {
        CheckStatus::NotFetched => (format!("{}: not fetched", report.citation_key), Color::Yellow),
        CheckStatus::Reproducible => (format!("{}: reproducible", report.citation_key), Color::Green),
        CheckStatus::Mismatch { expected, actual } => (
            format!("{}: MISMATCH (expected {expected}, got {actual})", report.citation_key),
            Color::Red,
        ),
        CheckStatus::Error(msg) => (format!("{}: ERROR — {msg}", report.citation_key), Color::Red),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pax_core::PaxError;

    #[test]
    fn sync_line_formats_fetched() {
        let report = SyncReport {
            citation_key: "turing1936".to_string(),
            result: Ok(FetchOutcome::Fetched {
                hash: "sha256-abc".to_string(),
            }),
        };
        let (text, color) = sync_line(&report);
        assert_eq!(text, "turing1936: fetched (hash sha256-abc)");
        assert_eq!(color, Color::Green);
    }

    #[test]
    fn sync_line_formats_already_fetched() {
        let report = SyncReport {
            citation_key: "turing1936".to_string(),
            result: Ok(FetchOutcome::AlreadyFetched {
                hash: "sha256-abc".to_string(),
            }),
        };
        let (text, _) = sync_line(&report);
        assert_eq!(text, "turing1936: already fetched (hash sha256-abc)");
    }

    #[test]
    fn sync_line_formats_error() {
        let report = SyncReport {
            citation_key: "turing1936".to_string(),
            result: Err(PaxError::NoSourceUrl("turing1936".to_string())),
        };
        let (text, color) = sync_line(&report);
        assert!(text.starts_with("turing1936: ERROR —"));
        assert_eq!(color, Color::Red);
    }

    #[test]
    fn check_line_formats_every_status() {
        let cases = [
            (CheckStatus::NotFetched, "turing1936: not fetched", Color::Yellow),
            (CheckStatus::Reproducible, "turing1936: reproducible", Color::Green),
            (
                CheckStatus::Mismatch {
                    expected: "sha256-abc".to_string(),
                    actual: "sha256-xyz".to_string(),
                },
                "turing1936: MISMATCH (expected sha256-abc, got sha256-xyz)",
                Color::Red,
            ),
            (
                CheckStatus::Error("connection refused".to_string()),
                "turing1936: ERROR — connection refused",
                Color::Red,
            ),
        ];
        for (status, expected_text, expected_color) in cases {
            let report = CheckReport {
                citation_key: "turing1936".to_string(),
                status,
            };
            let (text, color) = check_line(&report);
            assert_eq!(text, expected_text);
            assert_eq!(color, expected_color);
        }
    }
}
