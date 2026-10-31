use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pax_core::{
    CandidateId, CheckReport, CitationError, Exclusion, ExclusionStage, FetchOutcome, LibraryIndex,
    LogOutcome, PaperEdits, PaperRef, PaxError, ProviderError, ProviderId, ResolvedArtifact,
    SearchPage, SyncReport,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::message::Message;
use crate::pax_ctx::PaxCtx;
use crate::query::{SearchKind, SearchRequest};

pub type JobId = u64;

/// Each provider's page, or why it has none, by provider: what the search
/// screen shows for a search and for a citation list alike.
pub type SearchResults = HashMap<ProviderId, Result<SearchPage, ProviderFailure>>;

/// One provider's failure, as the search screen shows it. A search fails
/// per provider with a `ProviderError` and a citation list with a
/// `CitationError`; the screen needs neither type, only the message and
/// whether the provider simply has no record of what it was asked about —
/// an answer, not a failure, which `pax refs` prints as a note rather than an
/// error. Converting once, in the job, keeps one `SearchResults` type for
/// both, so the screen, merging and load-more stay shared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderFailure {
    pub message: String,
    /// The provider has no record of the seed or DOI it was asked about.
    pub no_record: bool,
}

impl std::fmt::Display for ProviderFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<ProviderError> for ProviderFailure {
    fn from(e: ProviderError) -> Self {
        ProviderFailure {
            no_record: matches!(e, ProviderError::NotFound),
            message: e.to_string(),
        }
    }
}

impl From<CitationError> for ProviderFailure {
    /// No record when the CLI's `pax refs` counts it as not found: the seed
    /// isn't known to this provider, or fetching it to learn its DOI found
    /// no such work. A failed request stays a failure.
    fn from(e: CitationError) -> Self {
        let no_record = match &e {
            CitationError::Unresolvable {
                lookup_error: Some(lookup),
                ..
            } => matches!(lookup.error, ProviderError::NotFound),
            CitationError::Unresolvable { .. } | CitationError::SeedNotFound { .. } => true,
            CitationError::Provider(e) => matches!(e, ProviderError::NotFound),
            _ => false,
        };
        ProviderFailure {
            message: e.to_string(),
            no_record,
        }
    }
}

/// `results` with each provider's error converted for display.
fn for_display<E: Into<ProviderFailure>>(
    results: HashMap<ProviderId, Result<SearchPage, E>>,
) -> SearchResults {
    results
        .into_iter()
        .map(|(provider, result)| (provider, result.map_err(Into::into)))
        .collect()
}

/// The only module allowed to call into `pax_core` directly — every other
/// module works with `pax_core` *types* (`Paper`, `PaxError`, ...) but never
/// calls its functions itself. Keeps "never shells out / hand-parses
/// papers.nix" auditable in one place.
pub enum JobKind {
    LoadLibrary,
    Search(SearchRequest),
    AddCandidate(CandidateId),
    FetchPaper(String),
    ResolveForOpen(String),
    EditPaper {
        citation_key: String,
        edits: PaperEdits,
    },
    RemovePaper(String),
    Sync,
    Check,
    ExportToFile {
        path: PathBuf,
        content: String,
    },
    Init,
    UploadPdf {
        citation_key: String,
        local_path: PathBuf,
    },
    Stats,
    Exclude(ExcludeRequest),
}

/// A search result to record in `research/excluded.nix`, through
/// `pax_core::exclude_paper`: its representative record's `provider:id`
/// and title (passed along so the exclusion has one even if the provider
/// can't be reached), with the user's reason and stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludeRequest {
    pub reference: String,
    pub title: String,
    pub reason: String,
    pub stage: ExclusionStage,
}

pub enum JobOutcome {
    Library(Result<Vec<pax_core::Paper>, PaxError>),
    Searched {
        // Boxed: these dwarf every other outcome, and a search finishes
        // rarely enough that the allocation is free.
        request: Box<SearchRequest>,
        results: SearchResults,
        index: Box<pax_core::LibraryIndex>,
        /// Whether the search was recorded in `research/search-log.jsonl`;
        /// `None` for a DOI lookup, which isn't logged.
        log: Option<LogOutcome>,
    },
    /// A citation list whose seed itself is invalid — a malformed reference
    /// or a citation key not in the library — so no provider was asked and
    /// nothing was logged.
    SeedFailed {
        request: Box<SearchRequest>,
        error: PaxError,
    },
    Added(Result<PaperRef, PaxError>),
    Fetched {
        citation_key: String,
        result: Result<FetchOutcome, PaxError>,
    },
    ReadyToOpen {
        citation_key: String,
        result: Result<PathBuf, OpenError>,
    },
    Edited {
        citation_key: String,
        result: Result<(), PaxError>,
    },
    Removed {
        citation_key: String,
        result: Result<(), PaxError>,
    },
    Synced(Result<Vec<SyncReport>, PaxError>),
    Checked(Result<Vec<CheckReport>, PaxError>),
    Exported {
        path: PathBuf,
        result: std::io::Result<()>,
    },
    /// `pax_core::init_library`'s real return type — `std::io::Error`, not
    /// `PaxError` (confirmed against its source, not assumed).
    Initialized(std::io::Result<()>),
    Uploaded {
        citation_key: String,
        /// The resulting release-asset URL on success — already persisted
        /// as the paper's `source_url` by the time this arrives (see
        /// `upload_pdf`), not just returned for display.
        result: Result<String, PaxError>,
    },
    Stats(Result<pax_core::Stats, PaxError>),
    /// The recorded exclusion, with the library index reloaded after it so
    /// the search's flags come from `excluded.nix` itself.
    Excluded {
        reference: String,
        result: Result<(Exclusion, Box<LibraryIndex>), PaxError>,
    },
}

/// Why a declared paper couldn't be resolved to an openable path. Mirrors
/// the `pax open` CLI's own error cases (`src/bin/pax/main.rs`'s
/// `Command::Open` arm).
pub enum OpenError {
    Pax(PaxError),
    NoSourceUrl(String),
    StillNotFetched,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::Pax(e) => write!(f, "{e}"),
            OpenError::NoSourceUrl(key) => write!(
                f,
                "{key:?} has no PDF source recorded — the provider found no open-access copy when it was added, so there's nothing to fetch"
            ),
            OpenError::StillNotFetched => {
                write!(f, "fetched, but the artifact still couldn't be resolved")
            }
        }
    }
}

impl From<PaxError> for OpenError {
    fn from(e: PaxError) -> Self {
        OpenError::Pax(e)
    }
}

pub fn spawn(id: JobId, kind: JobKind, ctx: PaxCtx, tx: UnboundedSender<Message>) {
    match kind {
        JobKind::LoadLibrary => {
            tokio::spawn(async move {
                let root = ctx.root.clone();
                let result = tokio::task::spawn_blocking(move || load_library(&root))
                    .await
                    .expect("load_library task panicked");
                let _ = tx.send(Message::Job(id, JobOutcome::Library(result)));
            });
        }
        JobKind::FetchPaper(citation_key) => {
            tokio::spawn(async move {
                let root = ctx.root.clone();
                let key = citation_key.clone();
                let result =
                    tokio::task::spawn_blocking(move || pax_core::fetch_paper(&key, &root))
                        .await
                        .expect("fetch_paper task panicked");
                let _ = tx.send(Message::Job(
                    id,
                    JobOutcome::Fetched {
                        citation_key,
                        result,
                    },
                ));
            });
        }
        JobKind::ResolveForOpen(citation_key) => {
            tokio::spawn(async move {
                let root = ctx.root.clone();
                let key = citation_key.clone();
                let result = tokio::task::spawn_blocking(move || resolve_for_open(&key, &root))
                    .await
                    .expect("resolve_for_open task panicked");
                let _ = tx.send(Message::Job(
                    id,
                    JobOutcome::ReadyToOpen {
                        citation_key,
                        result,
                    },
                ));
            });
        }
        JobKind::EditPaper {
            citation_key,
            edits,
        } => {
            tokio::spawn(async move {
                let root = ctx.root.clone();
                let key = citation_key.clone();
                let result =
                    tokio::task::spawn_blocking(move || pax_core::edit_paper(&key, &root, &edits))
                        .await
                        .expect("edit_paper task panicked");
                let _ = tx.send(Message::Job(
                    id,
                    JobOutcome::Edited {
                        citation_key,
                        result,
                    },
                ));
            });
        }
        JobKind::UploadPdf {
            citation_key,
            local_path,
        } => {
            tokio::spawn(async move {
                let root = ctx.root.clone();
                let key = citation_key.clone();
                let result =
                    tokio::task::spawn_blocking(move || upload_pdf(&key, &root, &local_path))
                        .await
                        .expect("upload_pdf task panicked");
                let _ = tx.send(Message::Job(
                    id,
                    JobOutcome::Uploaded {
                        citation_key,
                        result,
                    },
                ));
            });
        }
        JobKind::RemovePaper(citation_key) => {
            tokio::spawn(async move {
                let root = ctx.root.clone();
                let key = citation_key.clone();
                let result =
                    tokio::task::spawn_blocking(move || pax_core::remove_paper(&key, &root))
                        .await
                        .expect("remove_paper task panicked");
                let _ = tx.send(Message::Job(
                    id,
                    JobOutcome::Removed {
                        citation_key,
                        result,
                    },
                ));
            });
        }
        JobKind::Sync => {
            tokio::spawn(async move {
                let root = ctx.root.clone();
                let result = tokio::task::spawn_blocking(move || pax_core::sync_library(&root))
                    .await
                    .expect("sync_library task panicked");
                let _ = tx.send(Message::Job(id, JobOutcome::Synced(result)));
            });
        }
        JobKind::Check => {
            tokio::spawn(async move {
                let root = ctx.root.clone();
                let result = tokio::task::spawn_blocking(move || pax_core::check_library(&root))
                    .await
                    .expect("check_library task panicked");
                let _ = tx.send(Message::Job(id, JobOutcome::Checked(result)));
            });
        }
        JobKind::ExportToFile { path, content } => {
            tokio::spawn(async move {
                let p = path.clone();
                let result = tokio::task::spawn_blocking(move || std::fs::write(&p, content))
                    .await
                    .expect("export write task panicked");
                let _ = tx.send(Message::Job(id, JobOutcome::Exported { path, result }));
            });
        }
        JobKind::Init => {
            tokio::spawn(async move {
                let root = ctx.root.clone();
                let result = tokio::task::spawn_blocking(move || pax_core::init_library(&root))
                    .await
                    .expect("init_library task panicked");
                let _ = tx.send(Message::Job(id, JobOutcome::Initialized(result)));
            });
        }
        JobKind::Stats => {
            tokio::spawn(async move {
                let root = ctx.root.clone();
                let result = tokio::task::spawn_blocking(move || pax_core::stats(&root))
                    .await
                    .expect("stats task panicked");
                let _ = tx.send(Message::Job(id, JobOutcome::Stats(result)));
            });
        }
        network_kind @ (JobKind::Search(_) | JobKind::AddCandidate(_) | JobKind::Exclude(_)) => {
            tokio::spawn(async move {
                let outcome = run_network_job(network_kind, ctx).await;
                let _ = tx.send(Message::Job(id, outcome));
            });
        }
    }
}

/// Provider calls run as ordinary tasks on the main runtime's worker
/// threads: pax-core 2.0's search and `add_candidate` futures are `Send`,
/// so they never run on (or block) the event loop's own task.
async fn run_network_job(kind: JobKind, ctx: PaxCtx) -> JobOutcome {
    match kind {
        JobKind::Search(request) => search(request, &ctx).await,
        JobKind::AddCandidate(candidate_id) => {
            let result = pax_core::add_candidate(&candidate_id, &ctx.root, &ctx.config).await;
            JobOutcome::Added(result)
        }
        JobKind::Exclude(request) => exclude(request, &ctx).await,
        JobKind::LoadLibrary
        | JobKind::FetchPaper(_)
        | JobKind::ResolveForOpen(_)
        | JobKind::EditPaper { .. }
        | JobKind::RemovePaper(_)
        | JobKind::Sync
        | JobKind::Check
        | JobKind::ExportToFile { .. }
        | JobKind::Init
        | JobKind::UploadPdf { .. }
        | JobKind::Stats => {
            unreachable!("run_network_job is only called with network JobKinds")
        }
    }
}

/// Runs `request` the way `pax search`, `pax refs` and `pax cited-by` do:
/// free-text and author searches and citation lists through the logged,
/// paged entry points; a DOI lookup unpaged and unlogged, each provider's
/// records wrapped as a page with no total.
async fn search(request: SearchRequest, ctx: &PaxCtx) -> JobOutcome {
    let (root, config, options) = (&ctx.root, &ctx.config, &request.options);
    let (results, log) = match request.kind {
        SearchKind::Free => {
            let logged = pax_core::search_all_logged(&request.query, options, root, config).await;
            (for_display(logged.results), Some(logged.log))
        }
        SearchKind::Author => {
            let logged =
                pax_core::search_by_author_logged(&request.query, options, root, config).await;
            (for_display(logged.results), Some(logged.log))
        }
        SearchKind::Doi => {
            let results = pax_core::search_by_doi(&request.query, config)
                .await
                .into_iter()
                .map(|(provider, result)| {
                    (
                        provider,
                        result.map(|works| SearchPage::new(works, None, None)),
                    )
                })
                .collect();
            (for_display(results), None)
        }
        SearchKind::References | SearchKind::CitedBy => {
            let logged = if request.kind == SearchKind::References {
                pax_core::references_logged(&request.query, options, root, config).await
            } else {
                pax_core::cited_by_logged(&request.query, options, root, config).await
            };
            match logged {
                Ok(logged) => (for_display(logged.results), Some(logged.log)),
                Err(error) => {
                    return JobOutcome::SeedFailed {
                        request: Box::new(request),
                        error,
                    };
                }
            }
        }
    };
    JobOutcome::Searched {
        request: Box::new(request),
        results,
        index: Box::new(pax_core::LibraryIndex::load(root)),
        log,
    }
}

/// Excludes the paper, then reloads the library index the search flags its
/// rows from. `exclude_paper` resolves the reference with its provider, so
/// this runs as a network job, never on the UI thread.
async fn exclude(request: ExcludeRequest, ctx: &PaxCtx) -> JobOutcome {
    let result = pax_core::exclude_paper(
        &request.reference,
        &request.reason,
        request.stage,
        Some(&request.title),
        &ctx.root,
        &ctx.config,
    )
    .await
    .map(|exclusion| (exclusion, Box::new(LibraryIndex::load(&ctx.root))));
    JobOutcome::Excluded {
        reference: request.reference,
        result,
    }
}

fn load_library(root: &Path) -> Result<Vec<pax_core::Paper>, PaxError> {
    let path = pax_core::nix::papers_path(root);
    let library = pax_core::Library::load(&path)?;
    Ok(library.papers().to_vec())
}

/// Mirrors `pax open`'s own resolve -> fetch-if-needed -> resolve sequence
/// exactly (`src/bin/pax/main.rs`'s `Command::Open` arm), so lazypax's
/// "open" produces the same artifact-resolution behavior as the CLI: a
/// not-yet-fetched paper is fetched automatically and silently, without an
/// interactive prompt.
fn resolve_for_open(citation_key: &str, root: &Path) -> Result<PathBuf, OpenError> {
    use ResolvedArtifact::*;
    let resolved = match pax_core::resolve_artifact_path(citation_key, root)? {
        NotFetched => {
            pax_core::fetch_paper(citation_key, root)?;
            pax_core::resolve_artifact_path(citation_key, root)?
        }
        other => other,
    };
    match resolved {
        Path(p) => Ok(p),
        NoSourceUrl => Err(OpenError::NoSourceUrl(citation_key.to_string())),
        NotFetched => Err(OpenError::StillNotFetched),
    }
}

/// Publishes `local_path` to GitHub Releases and immediately records the
/// resulting URL as `citation_key`'s source, as one job — so a successful
/// upload always leaves the library consistent (no "uploaded but forgot to
/// save" half-state for the UI to recover from).
fn upload_pdf(citation_key: &str, root: &Path, local_path: &Path) -> Result<String, PaxError> {
    let url = pax_core::publish_to_github_release(root, citation_key, local_path)?;
    let edits = PaperEdits {
        source_url: Some(url.clone()),
        ..Default::default()
    };
    pax_core::edit_paper(citation_key, root, &edits)?;
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pax_core::SeedLookupError;

    #[test]
    fn a_provider_error_keeps_its_message_and_not_found_is_no_record() {
        let failure = ProviderFailure::from(ProviderError::Request("timeout".to_string()));
        assert_eq!(failure.to_string(), "request failed: timeout");
        assert!(!failure.no_record);
        assert!(ProviderFailure::from(ProviderError::NotFound).no_record);
    }

    #[test]
    fn a_citation_error_is_no_record_only_when_the_provider_lacks_the_seed() {
        let not_found = ProviderFailure::from(CitationError::SeedNotFound {
            provider: ProviderId::OpenAlex,
            key: "doi:10.1/x".to_string(),
        });
        assert!(not_found.no_record);
        assert!(not_found.message.contains("has no record of the seed"));

        let unresolvable = CitationError::Unresolvable {
            provider: ProviderId::OpenAlex,
            lookup_error: None,
        };
        assert!(ProviderFailure::from(unresolvable).no_record);

        let lookup_failed = CitationError::Unresolvable {
            provider: ProviderId::OpenAlex,
            lookup_error: Some(SeedLookupError {
                provider: ProviderId::ArXiv,
                error: ProviderError::Request("503".to_string()),
            }),
        };
        assert!(!ProviderFailure::from(lookup_failed).no_record);

        let request = CitationError::Provider(ProviderError::Request("429".to_string()));
        assert!(!ProviderFailure::from(request).no_record);
    }

    #[test]
    fn for_display_converts_every_providers_error() {
        let mut results = HashMap::new();
        results.insert(
            ProviderId::OpenAlex,
            Ok(SearchPage::new(Vec::new(), Some(0), None)),
        );
        results.insert(
            ProviderId::SemanticScholar,
            Err(CitationError::Provider(ProviderError::NotFound)),
        );
        let shown = for_display(results);
        assert!(shown[&ProviderId::OpenAlex].is_ok());
        assert_eq!(
            shown[&ProviderId::SemanticScholar].as_ref().unwrap_err(),
            &ProviderFailure {
                message: "not found".to_string(),
                no_record: true,
            }
        );
    }
}
