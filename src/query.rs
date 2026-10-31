//! What a search asks pax-core for: the kind of search, the text, and the
//! `SearchOptions` (limit, filters) — kept apart from the search screen so
//! the job that runs it and the screen that shows it share one description.
//! A citation list (a seed's references or the papers citing it) is a
//! search too: its kind says which list, and its text is the seed.

use pax_core::{DocType, Filters, Limit, SearchOptions, YearRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Free,
    Author,
    Doi,
    /// The works the seed cites (backward snowballing).
    References,
    /// The works citing the seed (forward snowballing).
    CitedBy,
}

impl SearchKind {
    pub fn label(self) -> &'static str {
        match self {
            SearchKind::Free => "free",
            SearchKind::Author => "author",
            SearchKind::Doi => "doi",
            SearchKind::References => "references",
            SearchKind::CitedBy => "cited by",
        }
    }

    /// The next mode a typed query can search in. A citation list isn't one
    /// (it's opened from a paper, not typed), so cycling from it starts over
    /// at free text.
    pub fn next(self) -> Self {
        match self {
            SearchKind::Free => SearchKind::Author,
            SearchKind::Author => SearchKind::Doi,
            SearchKind::Doi | SearchKind::References | SearchKind::CitedBy => SearchKind::Free,
        }
    }

    /// Whether this kind lists a seed's citations rather than searching for
    /// a typed query.
    pub fn is_citations(self) -> bool {
        matches!(self, SearchKind::References | SearchKind::CitedBy)
    }
}

/// One search as dispatched to pax-core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub kind: SearchKind,
    /// The query, author name or DOI; for a citation list, the seed — a
    /// citation key or a `provider:id` reference, as `pax refs` takes it.
    pub query: String,
    /// The per-provider limit and filters. Ignored by a DOI lookup, which
    /// resolves a single record per provider (as `pax search --doi` does).
    pub options: SearchOptions,
    /// For a citation list, the seed's title, to name it on screen; the
    /// seed itself is what pax-core is asked about.
    pub seed_title: Option<String>,
}

impl SearchRequest {
    /// A search with pax-core's default limit (the same as `pax search`).
    pub fn new(kind: SearchKind, query: impl Into<String>) -> Self {
        SearchRequest {
            kind,
            query: query.into(),
            options: SearchOptions::default(),
            seed_title: None,
        }
    }

    /// The `kind` citation list (references or cited by) of `seed`, a
    /// citation key or `provider:id` reference, named `title` on screen.
    pub fn citations(kind: SearchKind, seed: impl Into<String>, title: impl Into<String>) -> Self {
        debug_assert!(kind.is_citations());
        SearchRequest {
            seed_title: Some(title.into()),
            ..SearchRequest::new(kind, seed)
        }
    }

    /// What a citation list shows, for the search screen's header:
    /// `references of "Title" (seed)`; `None` for a typed search.
    pub fn citations_label(&self) -> Option<String> {
        let what = match self.kind {
            SearchKind::References => "references of",
            SearchKind::CitedBy => "cited by",
            SearchKind::Free | SearchKind::Author | SearchKind::Doi => return None,
        };
        Some(match &self.seed_title {
            Some(title) => format!("{what} \"{title}\" ({})", self.query),
            None => format!("{what} {}", self.query),
        })
    }

    /// The same search restricted by `filters`.
    pub fn with_filters(self, filters: Filters) -> Self {
        SearchRequest {
            options: self.options.with_filters(filters),
            ..self
        }
    }

    /// Whether this search pages a result set — every kind but a DOI lookup,
    /// citation lists included.
    pub fn is_paged(&self) -> bool {
        self.kind != SearchKind::Doi
    }

    /// The same search with each provider's limit doubled; `None` once it
    /// already retrieves everything. pax-core's paged search has no offset,
    /// so loading more re-runs the search from the start: doubling keeps the
    /// total work of repeated presses linear in the results finally shown.
    pub fn more(&self) -> Option<Self> {
        match self.options.limit {
            Limit::Count(n) => Some(self.with_limit(Limit::Count(n.saturating_mul(2).max(1)))),
            _ => None,
        }
    }

    /// The same search retrieving every result each provider serves; `None`
    /// once it already does.
    pub fn all(&self) -> Option<Self> {
        match self.options.limit {
            Limit::All => None,
            _ => Some(self.with_limit(Limit::All)),
        }
    }

    fn with_limit(&self, limit: Limit) -> Self {
        SearchRequest {
            options: self.options.clone().with_limit(limit),
            ..self.clone()
        }
    }

    /// The per-provider limit, for display: `"50"` or `"all"`.
    pub fn limit_label(&self) -> String {
        match self.options.limit {
            Limit::Count(n) => n.to_string(),
            _ => "all".to_string(),
        }
    }
}

/// Parses the filter prompt: space-separated `year:`, `type:` and `lang:`
/// terms, each value checked by pax-core's own parser, so it accepts what
/// `pax search --year/--type/--lang` accept. An empty prompt clears every
/// filter.
pub fn parse_filters(text: &str) -> Result<Filters, String> {
    let mut filters = Filters::default();
    for term in text.split_whitespace() {
        let Some((key, value)) = term.split_once(':') else {
            return Err(format!(
                "{term:?} isn't a filter: use year:YYYY[-YYYY], type:TYPE or lang:CODE"
            ));
        };
        filters = match key {
            "year" => filters.with_years(value.parse::<YearRange>().map_err(|e| e.to_string())?),
            "type" => filters.with_doc_type(value.parse::<DocType>().map_err(|e| e.to_string())?),
            "lang" if !value.is_empty() => filters.with_language(value),
            "lang" => return Err("lang: needs an ISO 639-1 code, e.g. lang:en".to_string()),
            _ => {
                return Err(format!("unknown filter {key:?}: use year:, type: or lang:"));
            }
        };
    }
    Ok(filters)
}

/// `filters` in the syntax [`parse_filters`] reads, so the prompt opens with
/// the filters already set; empty when none are.
pub fn format_filters(filters: &Filters) -> String {
    let mut terms = Vec::new();
    if let Some(years) = &filters.years {
        terms.push(format!("year:{years}"));
    }
    if let Some(doc_type) = &filters.doc_type {
        terms.push(format!("type:{doc_type}"));
    }
    if let Some(language) = &filters.language {
        terms.push(format!("lang:{language}"));
    }
    terms.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_parse_with_pax_cores_parsers_and_round_trip() {
        let filters = parse_filters(" year:2015-2020  type:journal-article lang:EN ").unwrap();
        assert_eq!(filters.years, YearRange::new(Some(2015), Some(2020)));
        assert_eq!(filters.doc_type, Some(DocType::JournalArticle));
        assert_eq!(filters.language.as_deref(), Some("en"));
        assert_eq!(
            format_filters(&filters),
            "year:2015-2020 type:journal-article lang:en"
        );
        assert_eq!(parse_filters(&format_filters(&filters)).unwrap(), filters);
        assert_eq!(
            parse_filters("year:-2000").unwrap().years,
            YearRange::new(None, Some(2000))
        );
    }

    #[test]
    fn an_empty_prompt_clears_the_filters() {
        assert_eq!(parse_filters("   ").unwrap(), Filters::default());
        assert_eq!(format_filters(&Filters::default()), "");
    }

    #[test]
    fn bad_filters_are_errors_naming_the_problem() {
        assert!(
            parse_filters("1999")
                .unwrap_err()
                .contains("isn't a filter")
        );
        assert!(
            parse_filters("venue:x")
                .unwrap_err()
                .contains("unknown filter")
        );
        assert!(parse_filters("year:99").is_err());
        assert!(
            parse_filters("type:poem")
                .unwrap_err()
                .contains("unknown document type")
        );
        assert!(parse_filters("lang:").is_err());
    }

    #[test]
    fn a_filtered_request_keeps_its_filters_when_loading_more() {
        let filters = parse_filters("year:2020").unwrap();
        let request = SearchRequest::new(SearchKind::Free, "q").with_filters(filters.clone());
        assert_eq!(request.more().unwrap().options.filters, filters);
        assert_eq!(request.all().unwrap().options.filters, filters);
    }

    #[test]
    fn cycle_kind_rotates_free_author_doi() {
        assert_eq!(SearchKind::Free.next(), SearchKind::Author);
        assert_eq!(SearchKind::Author.next(), SearchKind::Doi);
        assert_eq!(SearchKind::Doi.next(), SearchKind::Free);
    }

    #[test]
    fn cycling_from_a_citation_list_returns_to_free_text() {
        assert_eq!(SearchKind::References.next(), SearchKind::Free);
        assert_eq!(SearchKind::CitedBy.next(), SearchKind::Free);
        assert!(SearchKind::References.is_citations());
        assert!(!SearchKind::Free.is_citations());
    }

    #[test]
    fn a_citation_list_is_labelled_by_its_seed() {
        let refs = SearchRequest::citations(
            SearchKind::References,
            "turing1936",
            "On Computable Numbers",
        );
        assert_eq!(
            refs.citations_label().unwrap(),
            "references of \"On Computable Numbers\" (turing1936)"
        );
        let cited = SearchRequest {
            seed_title: None,
            ..SearchRequest::citations(SearchKind::CitedBy, "openalex:W1", "T")
        };
        assert_eq!(cited.citations_label().unwrap(), "cited by openalex:W1");
        assert!(
            SearchRequest::new(SearchKind::Free, "q")
                .citations_label()
                .is_none()
        );
    }

    #[test]
    fn a_citation_list_pages_and_loads_more_like_a_search() {
        let filters = parse_filters("year:2000-").unwrap();
        let cited = SearchRequest::citations(SearchKind::CitedBy, "openalex:W1", "T")
            .with_filters(filters.clone());
        assert!(cited.is_paged());
        assert_eq!(cited.options.limit, SearchOptions::default().limit);
        let more = cited.more().unwrap();
        assert_eq!(more.options.limit, Limit::Count(100));
        assert_eq!(more.kind, SearchKind::CitedBy);
        assert_eq!(more.query, "openalex:W1");
        assert_eq!(more.seed_title.as_deref(), Some("T"));
        assert_eq!(more.options.filters, filters);
        assert_eq!(cited.all().unwrap().options.limit, Limit::All);
    }

    #[test]
    fn a_new_request_uses_pax_cores_default_limit() {
        let request = SearchRequest::new(SearchKind::Free, "actor model");
        assert_eq!(request.options, SearchOptions::default());
    }

    #[test]
    fn more_doubles_the_limit_and_keeps_everything_else() {
        let request = SearchRequest::new(SearchKind::Author, "hewitt");
        let more = request.more().unwrap();
        assert_eq!(more.options.limit, Limit::Count(100));
        assert_eq!(more.kind, SearchKind::Author);
        assert_eq!(more.query, "hewitt");
        assert_eq!(more.more().unwrap().options.limit, Limit::Count(200));
    }

    #[test]
    fn all_asks_for_every_result_and_nothing_follows_it() {
        let all = SearchRequest::new(SearchKind::Free, "q").all().unwrap();
        assert_eq!(all.options.limit, Limit::All);
        assert!(all.all().is_none());
        assert!(all.more().is_none());
        assert_eq!(all.limit_label(), "all");
    }

    #[test]
    fn only_a_doi_lookup_is_unpaged() {
        assert!(SearchRequest::new(SearchKind::Free, "q").is_paged());
        assert!(SearchRequest::new(SearchKind::Author, "q").is_paged());
        assert!(!SearchRequest::new(SearchKind::Doi, "10.1/x").is_paged());
    }
}
