use pax_core::{Paper, PaperEdits};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use ratatui::text::{Line, Span};

use crate::app::InsertTarget;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Tags,
    Notes,
    Rename,
    Title,
    Authors,
    Year,
    Doi,
    SourceUrl,
}

const FIELD_ORDER: [EditField; 8] = [
    EditField::Tags,
    EditField::Notes,
    EditField::Rename,
    EditField::Title,
    EditField::Authors,
    EditField::Year,
    EditField::Doi,
    EditField::SourceUrl,
];

impl EditField {
    fn label(self) -> &'static str {
        match self {
            EditField::Tags => "Tags",
            EditField::Notes => "Notes",
            EditField::Rename => "Citation key",
            EditField::Title => "Title",
            EditField::Authors => "Authors",
            EditField::Year => "Year",
            EditField::Doi => "DOI",
            EditField::SourceUrl => "PDF source URL",
        }
    }

    /// Which buffer editing this field opens — `None` for `Tags`, which
    /// isn't a single value (`a`/`x` add/remove a tag directly instead).
    fn insert_target(self) -> Option<InsertTarget> {
        match self {
            EditField::Tags => None,
            EditField::Notes => Some(InsertTarget::EditNotes),
            EditField::Rename => Some(InsertTarget::EditRename),
            EditField::Title => Some(InsertTarget::EditTitle),
            EditField::Authors => Some(InsertTarget::EditAuthors),
            EditField::Year => Some(InsertTarget::EditYear),
            EditField::Doi => Some(InsertTarget::EditDoi),
            EditField::SourceUrl => Some(InsertTarget::EditSourceUrl),
        }
    }
}

pub struct EditScreen {
    pub citation_key: String,
    pub tags: Vec<String>,
    original_tags: Vec<String>,
    pub notes: String,
    pub rename: String,
    pub title: String,
    pub authors: String,
    pub year: String,
    pub doi: String,
    pub source_url: String,
    /// Snapshots of the single-value fields as loaded, so `build_edits` can
    /// tell "the user actually changed this" from "this field is pre-filled
    /// with the paper's existing, necessarily non-empty value" (title and
    /// authors are never empty on a real paper, so an empty-ness check
    /// alone would treat every save as touching them).
    original_notes: String,
    original_title: String,
    original_authors: String,
    original_year: String,
    original_doi: String,
    original_source_url: String,
    pub focus: EditField,
    /// Live edit buffer while an `InsertTarget::Edit*` mode is active; only
    /// copied into the target field on submit. Shared across all Edit
    /// fields since only one can be active at a time.
    pub buffer: String,
}

impl Default for EditScreen {
    fn default() -> Self {
        Self {
            citation_key: String::new(),
            tags: Vec::new(),
            original_tags: Vec::new(),
            notes: String::new(),
            rename: String::new(),
            title: String::new(),
            authors: String::new(),
            year: String::new(),
            doi: String::new(),
            source_url: String::new(),
            original_notes: String::new(),
            original_title: String::new(),
            original_authors: String::new(),
            original_year: String::new(),
            original_doi: String::new(),
            original_source_url: String::new(),
            focus: EditField::Tags,
            buffer: String::new(),
        }
    }
}

impl EditScreen {
    pub fn from_paper(paper: &Paper) -> Self {
        let notes = paper.local.notes.clone().unwrap_or_default();
        let title = paper.identity.title.clone();
        let authors = paper.identity.authors.join(", ");
        let year = paper.identity.year.map(|y| y.to_string()).unwrap_or_default();
        let doi = paper.identity.doi.clone().unwrap_or_default();
        let source_url = paper.artifact.source_url.clone().unwrap_or_default();
        Self {
            citation_key: paper.local.citation_key.clone(),
            tags: paper.local.tags.clone(),
            original_tags: paper.local.tags.clone(),
            rename: paper.local.citation_key.clone(),
            original_notes: notes.clone(),
            original_title: title.clone(),
            original_authors: authors.clone(),
            original_year: year.clone(),
            original_doi: doi.clone(),
            original_source_url: source_url.clone(),
            notes,
            title,
            authors,
            year,
            doi,
            source_url,
            focus: EditField::Tags,
            buffer: String::new(),
        }
    }

    pub fn focus_next(&mut self) {
        let idx = FIELD_ORDER.iter().position(|f| *f == self.focus).unwrap_or(0);
        self.focus = FIELD_ORDER[(idx + 1) % FIELD_ORDER.len()];
    }

    pub fn focus_prev(&mut self) {
        let idx = FIELD_ORDER.iter().position(|f| *f == self.focus).unwrap_or(0);
        self.focus = FIELD_ORDER[(idx + FIELD_ORDER.len() - 1) % FIELD_ORDER.len()];
    }

    pub fn go_top(&mut self) {
        self.focus = EditField::Tags;
    }

    pub fn go_bottom(&mut self) {
        self.focus = EditField::SourceUrl;
    }

    /// The `InsertTarget` for whichever field currently has focus —
    /// `None` for `Tags`.
    pub fn focused_insert_target(&self) -> Option<InsertTarget> {
        self.focus.insert_target()
    }

    pub fn remove_last_tag(&mut self) {
        self.tags.pop();
    }

    /// Reflects a source URL that's already been persisted elsewhere — the
    /// upload job calls `pax_core::edit_paper` itself, so this just syncs
    /// the Edit screen's display. Sets both the live and "original" copies
    /// so `build_edits` doesn't also try to resubmit it as a pending change
    /// on the next save.
    pub fn apply_uploaded_source_url(&mut self, url: String) {
        self.original_source_url = url.clone();
        self.source_url = url;
    }

    pub fn begin_field_edit(&mut self, target: InsertTarget) {
        self.buffer = match target {
            InsertTarget::EditTagAdd => String::new(),
            InsertTarget::EditNotes => self.notes.clone(),
            InsertTarget::EditRename => self.rename.clone(),
            InsertTarget::EditTitle => self.title.clone(),
            InsertTarget::EditAuthors => self.authors.clone(),
            InsertTarget::EditYear => self.year.clone(),
            InsertTarget::EditDoi => self.doi.clone(),
            InsertTarget::EditSourceUrl => self.source_url.clone(),
            InsertTarget::UploadPdfPath
            | InsertTarget::LibraryFilter
            | InsertTarget::SearchQuery
            | InsertTarget::ExportPath => String::new(),
        };
    }

    pub fn commit_field_edit(&mut self, target: InsertTarget) {
        match target {
            InsertTarget::EditTagAdd => {
                let tag = self.buffer.trim().to_string();
                if !tag.is_empty() && !self.tags.contains(&tag) {
                    self.tags.push(tag);
                }
            }
            InsertTarget::EditNotes => self.notes = self.buffer.clone(),
            InsertTarget::EditRename => self.rename = self.buffer.clone(),
            InsertTarget::EditTitle => self.title = self.buffer.clone(),
            InsertTarget::EditAuthors => self.authors = self.buffer.clone(),
            InsertTarget::EditYear => self.year = self.buffer.clone(),
            InsertTarget::EditDoi => self.doi = self.buffer.clone(),
            InsertTarget::EditSourceUrl => self.source_url = self.buffer.clone(),
            // `UploadPdfPath` never reaches here — `App::submit_input`
            // intercepts it before falling through to `commit_field_edit`,
            // since submitting it dispatches an upload job rather than
            // writing straight into a field. Still handled explicitly
            // (as a no-op) to keep this match exhaustive.
            InsertTarget::UploadPdfPath | InsertTarget::LibraryFilter | InsertTarget::SearchQuery | InsertTarget::ExportPath => {}
        }
        self.buffer.clear();
    }

    /// Builds the `PaperEdits` to send to `pax_core::edit_paper`. Every
    /// field is opt-in (matches `PaperEdits`'s own contract): a field is
    /// included only if it actually differs from what the paper was loaded
    /// with, and never as an empty value — none of `pax_core`'s
    /// single-value fields support clearing back to `None` anyway (a
    /// pre-existing `pax-core` limitation, not one this screen adds), so
    /// clearing a field back to empty here is treated as "no change" rather
    /// than sent as a blank overwrite. Tags are the exception: they're
    /// diffed against the paper's original tags to produce incremental
    /// add/remove lists, matching `PaperEdits`'s own incremental
    /// (not full-replace) tags API.
    pub fn build_edits(&self) -> PaperEdits {
        let add_tags = self
            .tags
            .iter()
            .filter(|t| !self.original_tags.contains(t))
            .cloned()
            .collect();
        let remove_tags = self
            .original_tags
            .iter()
            .filter(|t| !self.tags.contains(t))
            .cloned()
            .collect();
        let notes = changed(&self.original_notes, &self.notes);
        let rename = if !self.rename.is_empty() && self.rename != self.citation_key {
            Some(self.rename.clone())
        } else {
            None
        };
        let title = changed(&self.original_title, &self.title);
        let authors = if self.authors != self.original_authors {
            let parsed: Vec<String> = self
                .authors
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if parsed.is_empty() { None } else { Some(parsed) }
        } else {
            None
        };
        let year = if self.year != self.original_year {
            self.year.trim().parse::<i32>().ok()
        } else {
            None
        };
        let doi = changed(&self.original_doi, &self.doi);
        let source_url = changed(&self.original_source_url, &self.source_url);

        PaperEdits {
            add_tags,
            remove_tags,
            notes,
            rename,
            title,
            authors,
            year,
            doi,
            source_url,
        }
    }
}

/// `Some(current)` only if it differs from `original` and isn't empty —
/// an emptied field is treated as "leave unchanged" (see `build_edits`).
fn changed(original: &str, current: &str) -> Option<String> {
    if current != original && !current.is_empty() {
        Some(current.to_string())
    } else {
        None
    }
}

pub fn draw(frame: &mut Frame, screen: &EditScreen, editing_target: Option<InsertTarget>, area: Rect) {
    let mut lines = vec![Line::from(format!("Editing {}", screen.citation_key))];
    // `UploadPdfPath` has no corresponding `EditField` (it isn't a
    // persistent value on the paper, only the local path prompt for one
    // upload), so it can't be shown by the per-field loop below like every
    // other insert target — it needs its own line, or the buffer being
    // typed is invisible.
    if editing_target == Some(InsertTarget::UploadPdfPath) {
        lines.push(Line::from(Span::styled(
            format!("Upload PDF path: {}▏", screen.buffer),
            Style::default().add_modifier(Modifier::BOLD),
        )));
    }
    for field in FIELD_ORDER {
        let focused = field == screen.focus;
        let value = field_display(screen, field, editing_target);
        let prefix = if focused { "> " } else { "  " };
        let style = if focused {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(
            format!("{prefix}{}: {value}", field.label()),
            style,
        )));
    }
    let block = Block::default().title(" lazypax — edit ").borders(Borders::ALL);
    frame.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), area);
}

fn field_display(screen: &EditScreen, field: EditField, editing_target: Option<InsertTarget>) -> String {
    if field == EditField::Tags {
        return if screen.tags.is_empty() {
            "(none)".to_string()
        } else {
            screen.tags.join(", ")
        };
    }
    let is_being_edited = editing_target.is_some() && field.insert_target() == editing_target;
    if is_being_edited {
        return format!("{}▏", screen.buffer);
    }
    let value = match field {
        EditField::Notes => &screen.notes,
        EditField::Rename => &screen.rename,
        EditField::Title => &screen.title,
        EditField::Authors => &screen.authors,
        EditField::Year => &screen.year,
        EditField::Doi => &screen.doi,
        EditField::SourceUrl => &screen.source_url,
        EditField::Tags => unreachable!(),
    };
    if value.is_empty() {
        "(none)".to_string()
    } else {
        value.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pax_core::{Artifact, Identity, Local};

    fn paper() -> Paper {
        Paper {
            identity: Identity {
                doi: Some("10.1/x".to_string()),
                title: "A Title".to_string(),
                authors: vec!["Alice".to_string(), "Bob".to_string()],
                year: Some(2020),
                venue: None,
            },
            artifact: Artifact::default(),
            local: Local {
                citation_key: "alice2020".to_string(),
                tags: vec!["a".to_string(), "b".to_string()],
                notes: Some("existing notes".to_string()),
            },
        }
    }

    #[test]
    fn from_paper_prefills_every_field() {
        let screen = EditScreen::from_paper(&paper());
        assert_eq!(screen.citation_key, "alice2020");
        assert_eq!(screen.tags, vec!["a", "b"]);
        assert_eq!(screen.notes, "existing notes");
        assert_eq!(screen.rename, "alice2020");
        assert_eq!(screen.title, "A Title");
        assert_eq!(screen.authors, "Alice, Bob");
        assert_eq!(screen.year, "2020");
        assert_eq!(screen.doi, "10.1/x");
        assert_eq!(screen.source_url, ""); // most added papers have no PDF source yet
    }

    #[test]
    fn focus_wraps_forward_and_backward() {
        let mut screen = EditScreen::default();
        assert!(matches!(screen.focus, EditField::Tags));
        screen.focus_prev();
        assert!(matches!(screen.focus, EditField::SourceUrl));
        screen.focus_next();
        assert!(matches!(screen.focus, EditField::Tags));
    }

    #[test]
    fn no_changes_produces_empty_edits() {
        let screen = EditScreen::from_paper(&paper());
        let edits = screen.build_edits();
        assert!(edits.add_tags.is_empty());
        assert!(edits.remove_tags.is_empty());
        assert!(edits.notes.is_none());
        assert!(edits.rename.is_none());
        assert!(edits.title.is_none());
        assert!(edits.authors.is_none());
        assert!(edits.year.is_none());
        assert!(edits.doi.is_none());
        assert!(edits.source_url.is_none());
    }

    #[test]
    fn tag_changes_diff_against_the_original() {
        let mut screen = EditScreen::from_paper(&paper());
        screen.tags = vec!["a".to_string(), "c".to_string()]; // removed "b", added "c"
        let edits = screen.build_edits();
        assert_eq!(edits.add_tags, vec!["c".to_string()]);
        assert_eq!(edits.remove_tags, vec!["b".to_string()]);
    }

    #[test]
    fn rename_is_none_unless_actually_changed() {
        let mut screen = EditScreen::from_paper(&paper());
        assert!(screen.build_edits().rename.is_none());
        screen.rename = "alice2020new".to_string();
        assert_eq!(screen.build_edits().rename, Some("alice2020new".to_string()));
    }

    #[test]
    fn single_value_fields_only_change_when_edited() {
        let mut screen = EditScreen::from_paper(&paper());
        screen.title = "New Title".to_string();
        let edits = screen.build_edits();
        assert_eq!(edits.title, Some("New Title".to_string()));
        assert!(edits.year.is_none()); // untouched
    }

    #[test]
    fn setting_a_source_url_on_a_paper_that_had_none_produces_an_edit() {
        // Most added papers have no PDF source recorded (a provider found no
        // open-access copy); this is how the user supplies one after the fact.
        let mut screen = EditScreen::from_paper(&paper());
        assert_eq!(screen.source_url, "");
        screen.source_url = "https://example.org/paper.pdf".to_string();
        let edits = screen.build_edits();
        assert_eq!(edits.source_url, Some("https://example.org/paper.pdf".to_string()));
    }

    #[test]
    fn apply_uploaded_source_url_does_not_reappear_as_a_pending_edit() {
        // The upload job already calls `edit_paper` itself — this just
        // syncs the display, so a later `w` (save) with nothing else
        // touched must not resubmit the same URL as a "change".
        let mut screen = EditScreen::from_paper(&paper());
        screen.apply_uploaded_source_url("https://github.com/x/y/releases/download/papers/alice2020.pdf".to_string());
        assert_eq!(screen.source_url, "https://github.com/x/y/releases/download/papers/alice2020.pdf");
        assert!(screen.build_edits().source_url.is_none());
    }

    #[test]
    fn clearing_a_field_to_empty_is_treated_as_unchanged_not_a_blank_overwrite() {
        // pax-core has no way to clear a field back to null; sending an
        // empty string would silently blank it instead, which is worse
        // than just refusing the "clear" — so an emptied buffer must not
        // appear in the built edits at all.
        let mut screen = EditScreen::from_paper(&paper());
        screen.title.clear();
        screen.notes.clear();
        screen.doi.clear();
        let edits = screen.build_edits();
        assert!(edits.title.is_none());
        assert!(edits.notes.is_none());
        assert!(edits.doi.is_none());
    }

    #[test]
    fn authors_split_on_comma_and_trim() {
        let mut screen = EditScreen::from_paper(&paper());
        screen.authors = " Carol ,  Dave".to_string();
        let edits = screen.build_edits();
        assert_eq!(edits.authors, Some(vec!["Carol".to_string(), "Dave".to_string()]));
    }

    #[test]
    fn unparseable_year_is_treated_as_unchanged() {
        let mut screen = EditScreen::from_paper(&paper());
        screen.year = "not a year".to_string();
        assert!(screen.build_edits().year.is_none());
    }

    #[test]
    fn field_edit_round_trips_through_begin_and_commit() {
        let mut screen = EditScreen::from_paper(&paper());
        screen.begin_field_edit(InsertTarget::EditTitle);
        assert_eq!(screen.buffer, "A Title");
        screen.buffer = "Updated Title".to_string();
        screen.commit_field_edit(InsertTarget::EditTitle);
        assert_eq!(screen.title, "Updated Title");
        assert_eq!(screen.buffer, "");
    }

    #[test]
    fn tag_add_buffer_starts_empty_regardless_of_existing_tags() {
        let mut screen = EditScreen::from_paper(&paper());
        screen.begin_field_edit(InsertTarget::EditTagAdd);
        assert_eq!(screen.buffer, "");
    }

    #[test]
    fn committing_a_duplicate_tag_is_a_no_op() {
        let mut screen = EditScreen::from_paper(&paper());
        screen.buffer = "a".to_string();
        screen.commit_field_edit(InsertTarget::EditTagAdd);
        assert_eq!(screen.tags, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn remove_last_tag_pops_the_most_recent() {
        let mut screen = EditScreen::from_paper(&paper());
        screen.remove_last_tag();
        assert_eq!(screen.tags, vec!["a".to_string()]);
    }
}
