use pax_core::Stats;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

const TITLE: &str = " lazypax — PRISMA stats ";

/// The read-only PRISMA counts screen: whatever `pax_core::stats` last
/// returned for the library, recomputed every time the screen is opened —
/// nothing is cached past that.
#[derive(Default)]
pub struct StatsScreen {
    pub state: StatsState,
    /// First visible row, for libraries whose per-provider/per-reason
    /// breakdown runs past the screen.
    pub scroll: usize,
}

#[derive(Default)]
pub enum StatsState {
    #[default]
    Loading,
    Loaded(Stats),
    /// `pax_core::stats` failed (e.g. a malformed search-log line); the
    /// error's text, shown in place of the counts.
    Failed(String),
}

impl StatsScreen {
    /// Every row the screen shows, before scrolling: the skipped-lines
    /// warning (if any), then the PRISMA phases.
    pub fn lines(&self) -> Vec<Line<'static>> {
        match &self.state {
            StatsState::Loading => vec![Line::from("Computing PRISMA counts…")],
            StatsState::Failed(message) => vec![Line::styled(
                format!("Could not compute PRISMA counts: {message}"),
                Style::default().fg(Color::Red),
            )],
            StatsState::Loaded(stats) => {
                let mut lines = Vec::new();
                if let Some(warning) = skipped_warning(stats) {
                    lines.push(Line::styled(warning, Style::default().fg(Color::Yellow)));
                    lines.push(Line::default());
                }
                lines.extend(stats_rows(stats).into_iter().map(|row| match row {
                    StatsRow::Phase(phase) => {
                        Line::styled(phase, Style::default().add_modifier(Modifier::BOLD))
                    }
                    StatsRow::Count(text) => Line::from(text),
                }));
                lines
            }
        }
    }

    fn last_row(&self) -> usize {
        self.lines().len().saturating_sub(1)
    }

    pub fn scroll_down(&mut self) {
        self.scroll = (self.scroll + 1).min(self.last_row());
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_bottom(&mut self) {
        self.scroll = self.last_row();
    }
}

pub fn draw(frame: &mut Frame, screen: &StatsScreen, area: Rect) {
    let lines: Vec<Line> = screen.lines().into_iter().skip(screen.scroll).collect();
    let block = Block::default().title(TITLE).borders(Borders::ALL);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Mirrors `pax stats`' own warning (`sink.rs`), so the TUI says the same
/// thing the CLI prints to stderr.
fn skipped_warning(stats: &Stats) -> Option<String> {
    (stats.skipped_log_lines > 0).then(|| {
        format!(
            "Warning: skipped {} search-log line(s) whose provider failed, so retrieved no records",
            stats.skipped_log_lines
        )
    })
}

enum StatsRow {
    Phase(&'static str),
    Count(String),
}

/// The PRISMA 2020 boxes laid out exactly as `pax stats`' human output
/// (`sink.rs::stats_lines`): three phases, labels left-aligned with
/// per-provider and per-reason breakdowns indented under their box, counts
/// right-aligned in one column.
fn stats_rows(stats: &Stats) -> Vec<StatsRow> {
    enum Row {
        Phase(&'static str),
        Count(usize, String, usize),
    }
    let mut rows = Vec::new();
    let by_provider =
        |rows: &mut Vec<Row>, providers: &std::collections::BTreeMap<String, usize>| {
            for (provider, count) in providers {
                rows.push(Row::Count(2, provider.clone(), *count));
            }
        };
    let databases = &stats.identified.databases;
    let citation = &stats.identified.citation_searching;
    rows.push(Row::Phase("Identification"));
    rows.push(Row::Count(
        1,
        "Records identified from databases".into(),
        databases.values().sum(),
    ));
    by_provider(&mut rows, databases);
    rows.push(Row::Count(
        1,
        "Records identified from citation searching".into(),
        citation.values().sum(),
    ));
    by_provider(&mut rows, citation);
    rows.push(Row::Count(
        1,
        "Duplicate records removed".into(),
        stats.duplicates_removed,
    ));
    rows.push(Row::Phase("Screening"));
    for (label, count) in [
        ("Records screened", stats.records_screened),
        ("Records excluded", stats.excluded_screening),
        ("Reports sought for retrieval", stats.reports_sought),
        ("Reports not retrieved", stats.not_retrieved),
        ("Reports assessed for eligibility", stats.reports_assessed),
        ("Reports excluded", stats.excluded_full_text.total),
    ] {
        rows.push(Row::Count(1, label.into(), count));
    }
    for reason in &stats.excluded_full_text.reasons {
        rows.push(Row::Count(2, reason.reason.clone(), reason.count));
    }
    rows.push(Row::Phase("Included"));
    rows.push(Row::Count(
        1,
        "Studies included in review".into(),
        stats.included,
    ));

    let label_width = |indent: usize, label: &str| 2 * indent + label.chars().count();
    let counts = || {
        rows.iter().filter_map(|row| match row {
            Row::Count(indent, label, count) => Some((*indent, label, *count)),
            Row::Phase(_) => None,
        })
    };
    let width = counts()
        .map(|(indent, label, _)| label_width(indent, label))
        .max()
        .unwrap_or(0);
    let digits = counts()
        .map(|(_, _, count)| count.to_string().len())
        .max()
        .unwrap_or(1);
    rows.iter()
        .map(|row| match row {
            Row::Phase(phase) => StatsRow::Phase(phase),
            Row::Count(indent, label, count) => {
                let pad = width - label_width(*indent, label);
                StatsRow::Count(format!(
                    "{}{label}{}  {count:>digits$}",
                    "  ".repeat(*indent),
                    " ".repeat(pad)
                ))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::fs;
    use std::path::{Path, PathBuf};

    /// A scratch `research/` library, initialized by pax-core itself.
    fn fixture_library(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("lazy-pax-stats-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        pax_core::init_library(&root).unwrap();
        root
    }

    /// One search-log line in pax-core 2.0's schema; `keys` is `None` for a
    /// provider that failed.
    fn log_line(command: &str, provider: &str, keys: Option<&[&str]>) -> String {
        let input = match command {
            "refs" | "cited-by" => r#""seed":"lamport1978""#,
            _ => r#""query":"q""#,
        };
        let outcome = match keys {
            Some(keys) => {
                let records: Vec<Vec<&str>> = keys.iter().map(|key| vec![*key]).collect();
                format!(
                    r#""total":null,"retrieved":{},"keys":{records:?}"#,
                    records.len()
                )
            }
            None => r#""total":null,"error":"request failed""#.to_string(),
        };
        format!(
            r#"{{"v":1,"command":"{command}",{input},"provider":"{provider}","date":"2026-01-01T00:00:00Z","limit":50,"filters":[],{outcome},"merge":{{"before":0,"after":0,"unmatched":0}}}}"#
        )
    }

    fn write_log(root: &Path, lines: &[String]) {
        fs::write(
            root.join("research").join("search-log.jsonl"),
            lines.join("\n") + "\n",
        )
        .unwrap();
    }

    /// The README's worked example: 3 database records (one a duplicate of
    /// a citation-search record), 2 citation-search records, one screening
    /// and one full-text exclusion, one failed provider.
    fn populated_library(name: &str) -> PathBuf {
        let root = fixture_library(name);
        write_log(
            &root,
            &[
                log_line("search", "openalex", Some(&["doi:10.1/a", "doi:10.1/b"])),
                log_line("search", "arxiv", Some(&["doi:10.1/a"])),
                log_line("refs", "openalex", Some(&["doi:10.1/b", "doi:10.1/c"])),
                log_line("search", "semanticscholar", None),
            ],
        );
        fs::write(
            root.join("research").join("excluded.nix"),
            r#"{
  "10.1/a" = { title = "A"; reason = "off topic"; stage = "screening"; };
  "10.1/b" = { title = "B"; reason = "not empirical"; stage = "full-text"; };
}
"#,
        )
        .unwrap();
        root
    }

    fn loaded(root: &Path) -> StatsScreen {
        StatsScreen {
            state: StatsState::Loaded(pax_core::stats(root).unwrap()),
            scroll: 0,
        }
    }

    fn text(screen: &StatsScreen) -> Vec<String> {
        screen.lines().iter().map(ToString::to_string).collect()
    }

    fn render(screen: &StatsScreen, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| draw(frame, screen, frame.area()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_populated_library_mirrors_pax_stats_layout() {
        let root = populated_library("populated");
        let screen = loaded(&root);
        assert_eq!(
            text(&screen),
            [
                "Warning: skipped 1 search-log line(s) whose provider failed, so retrieved no records",
                "",
                "Identification",
                "  Records identified from databases           3",
                "    arxiv                                     1",
                "    openalex                                  2",
                "  Records identified from citation searching  2",
                "    openalex                                  2",
                "  Duplicate records removed                   2",
                "Screening",
                "  Records screened                            3",
                "  Records excluded                            1",
                "  Reports sought for retrieval                2",
                "  Reports not retrieved                       0",
                "  Reports assessed for eligibility            2",
                "  Reports excluded                            1",
                "    not empirical                             1",
                "Included",
                "  Studies included in review                  0",
            ]
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_fresh_library_shows_zeros_and_no_warning() {
        let root = fixture_library("fresh");
        let lines = text(&loaded(&root));
        assert_eq!(lines[0], "Identification");
        assert!(!lines.iter().any(|line| line.contains("Warning")));
        assert_eq!(
            lines.last().unwrap(),
            "  Studies included in review                  0"
        );
        assert!(
            lines
                .iter()
                .filter(|line| line.starts_with("  "))
                .all(|line| line.ends_with(" 0"))
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_malformed_search_log_renders_the_error_instead_of_counts() {
        let root = fixture_library("malformed");
        write_log(&root, &["{not json".to_string()]);
        let error = pax_core::stats(&root).unwrap_err();
        let screen = StatsScreen {
            state: StatsState::Failed(error.to_string()),
            scroll: 0,
        };
        let rendered = render(&screen, 120, 5);
        assert!(rendered.contains("PRISMA stats"), "{rendered}");
        assert!(
            rendered.contains("Could not compute PRISMA counts:"),
            "{rendered}"
        );
        assert!(!rendered.contains("Identification"), "{rendered}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rendering_shows_the_phases_inside_a_titled_block() {
        let root = populated_library("render");
        let rendered = render(&loaded(&root), 100, 25);
        assert!(rendered.contains("lazypax — PRISMA stats"), "{rendered}");
        assert!(
            rendered.contains("│Warning: skipped 1 search-log line(s)"),
            "{rendered}"
        );
        assert!(
            rendered.contains("│  Records identified from databases           3"),
            "{rendered}"
        );
        assert!(rendered.contains("│Included"), "{rendered}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scrolling_skips_rows_and_stops_at_the_last_one() {
        let root = populated_library("scroll");
        let mut screen = loaded(&root);
        let rows = screen.lines().len();
        screen.scroll_down();
        screen.scroll_down();
        assert_eq!(screen.scroll, 2);
        let rendered = render(&screen, 100, 25);
        assert!(!rendered.contains("Warning"), "{rendered}");
        assert!(rendered.contains("│Identification"), "{rendered}");
        screen.scroll_bottom();
        assert_eq!(screen.scroll, rows - 1);
        screen.scroll_down();
        assert_eq!(screen.scroll, rows - 1);
        screen.scroll_up();
        assert_eq!(screen.scroll, rows - 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn loading_and_scrolling_an_unloaded_screen_are_safe() {
        let mut screen = StatsScreen::default();
        screen.scroll_down();
        screen.scroll_bottom();
        assert_eq!(screen.scroll, 0);
        assert_eq!(text(&screen), ["Computing PRISMA counts…"]);
    }
}
