use pax_core::CandidateWork;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

pub enum DetailSubject {
    Candidate { work: CandidateWork, in_library: bool },
}

#[derive(Default)]
pub struct DetailScreen {
    pub subject: Option<DetailSubject>,
}

pub fn draw(frame: &mut Frame, screen: &DetailScreen, area: Rect) {
    let block = Block::default().title(" lazypax — detail ").borders(Borders::ALL);
    let text = match &screen.subject {
        None => "Nothing selected.".to_string(),
        Some(DetailSubject::Candidate { work, in_library }) => format_candidate(work, *in_library),
    };
    frame.render_widget(Paragraph::new(text).block(block).wrap(Wrap { trim: false }), area);
}

/// Mirrors the field set (and mostly the wording) of the `pax` CLI's own
/// `sink.rs::candidate()` display, so a paper looks the same whether you
/// inspected it with `pax show` or here.
fn format_candidate(work: &CandidateWork, in_library: bool) -> String {
    let authors = if work.authors.is_empty() {
        "—".to_string()
    } else {
        work.authors.join(", ")
    };
    let mut lines = vec![
        format!("Title:       {}", work.title),
        format!("Authors:     {authors}"),
        format!("Published:   {}", work.publish_date),
        format!("DOI:         {}", work.doi.as_deref().unwrap_or("(no doi)")),
        format!("Venue:       {}", work.venue.as_deref().unwrap_or("(no venue)")),
        format!(
            "PDF source:  {}",
            work.pdf_url.as_deref().unwrap_or("(none found)")
        ),
        format!("In library:  {}", if in_library { "yes" } else { "no" }),
        format!("Reference:   {}", work.id),
    ];
    if let Some(abstract_text) = &work.abstract_text {
        lines.push(String::new());
        lines.push(format!("Abstract:    {abstract_text}"));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pax_core::{CandidateId, ProviderId};

    fn work() -> CandidateWork {
        CandidateWork {
            id: CandidateId {
                provider: ProviderId::OpenAlex,
                native_id: "W123".to_string(),
            },
            title: "On Computable Numbers".to_string(),
            authors: vec!["Alan Turing".to_string()],
            publish_date: "1936-01-01".to_string(),
            doi: Some("10.1112/plms/s2-42.1.230".to_string()),
            pdf_url: Some("https://example.org/turing.pdf".to_string()),
            venue: Some("Proceedings of the LMS".to_string()),
            abstract_text: None,
        }
    }

    #[test]
    fn formats_known_fields_and_missing_ones_as_placeholders() {
        let mut w = work();
        w.doi = None;
        let text = format_candidate(&w, false);
        assert!(text.contains("Title:       On Computable Numbers"));
        assert!(text.contains("DOI:         (no doi)"));
        assert!(text.contains("In library:  no"));
        assert!(text.contains("Reference:   openalex:W123"));
    }

    #[test]
    fn shows_in_library_yes_when_flagged() {
        let text = format_candidate(&work(), true);
        assert!(text.contains("In library:  yes"));
    }

    #[test]
    fn includes_abstract_only_when_present() {
        let mut w = work();
        assert!(!format_candidate(&w, false).contains("Abstract:"));
        w.abstract_text = Some("A study of computability.".to_string());
        assert!(format_candidate(&w, false).contains("Abstract:    A study of computability."));
    }
}
