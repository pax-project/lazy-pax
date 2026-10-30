//! What a search asks pax-core for: the kind of search, the text, and the
//! `SearchOptions` (limit, filters) — kept apart from the search screen so
//! the job that runs it and the screen that shows it share one description.

use pax_core::{Limit, SearchOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Free,
    Author,
    Doi,
}

impl SearchKind {
    pub fn label(self) -> &'static str {
        match self {
            SearchKind::Free => "free",
            SearchKind::Author => "author",
            SearchKind::Doi => "doi",
        }
    }

    pub fn next(self) -> Self {
        match self {
            SearchKind::Free => SearchKind::Author,
            SearchKind::Author => SearchKind::Doi,
            SearchKind::Doi => SearchKind::Free,
        }
    }
}

/// One search as dispatched to pax-core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub kind: SearchKind,
    pub query: String,
    /// The per-provider limit and filters. Ignored by a DOI lookup, which
    /// resolves a single record per provider (as `pax search --doi` does).
    pub options: SearchOptions,
}

impl SearchRequest {
    /// A search with pax-core's default limit (the same as `pax search`).
    pub fn new(kind: SearchKind, query: impl Into<String>) -> Self {
        SearchRequest {
            kind,
            query: query.into(),
            options: SearchOptions::default(),
        }
    }

    /// Whether this search pages a result set — every kind but a DOI lookup.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_kind_rotates_free_author_doi() {
        assert_eq!(SearchKind::Free.next(), SearchKind::Author);
        assert_eq!(SearchKind::Author.next(), SearchKind::Doi);
        assert_eq!(SearchKind::Doi.next(), SearchKind::Free);
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
