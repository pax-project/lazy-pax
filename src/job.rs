use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pax_core::{
    CandidateId, CandidateWork, CheckReport, FetchOutcome, PaperEdits, PaperRef, PaxError, ProviderError, ProviderId,
    ResolvedArtifact, SyncReport,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::message::Message;
use crate::pax_ctx::PaxCtx;

pub type JobId = u64;

type SearchResults = HashMap<ProviderId, Result<Vec<CandidateWork>, ProviderError>>;

/// The only module allowed to call into `pax_core` directly — every other
/// module works with `pax_core` *types* (`Paper`, `PaxError`, ...) but never
/// calls its functions itself. Keeps "never shells out / hand-parses
/// papers.nix" auditable in one place.
pub enum JobKind {
    LoadLibrary,
    SearchAll(String),
    SearchByAuthor(String),
    SearchByDoi(String),
    AddCandidate(CandidateId),
    FetchPaper(String),
    ResolveForOpen(String),
    EditPaper { citation_key: String, edits: PaperEdits },
    RemovePaper(String),
    Sync,
    Check,
    ExportToFile { path: PathBuf, content: String },
}

pub enum JobOutcome {
    Library(Result<Vec<pax_core::Paper>, PaxError>),
    Searched {
        results: SearchResults,
        known_dois: std::collections::HashSet<String>,
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
            OpenError::StillNotFetched => write!(f, "fetched, but the artifact still couldn't be resolved"),
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
                let result = tokio::task::spawn_blocking(move || pax_core::fetch_paper(&key, &root))
                    .await
                    .expect("fetch_paper task panicked");
                let _ = tx.send(Message::Job(id, JobOutcome::Fetched { citation_key, result }));
            });
        }
        JobKind::ResolveForOpen(citation_key) => {
            tokio::spawn(async move {
                let root = ctx.root.clone();
                let key = citation_key.clone();
                let result = tokio::task::spawn_blocking(move || resolve_for_open(&key, &root))
                    .await
                    .expect("resolve_for_open task panicked");
                let _ = tx.send(Message::Job(id, JobOutcome::ReadyToOpen { citation_key, result }));
            });
        }
        JobKind::EditPaper { citation_key, edits } => {
            tokio::spawn(async move {
                let root = ctx.root.clone();
                let key = citation_key.clone();
                let result = tokio::task::spawn_blocking(move || pax_core::edit_paper(&key, &root, &edits))
                    .await
                    .expect("edit_paper task panicked");
                let _ = tx.send(Message::Job(id, JobOutcome::Edited { citation_key, result }));
            });
        }
        JobKind::RemovePaper(citation_key) => {
            tokio::spawn(async move {
                let root = ctx.root.clone();
                let key = citation_key.clone();
                let result = tokio::task::spawn_blocking(move || pax_core::remove_paper(&key, &root))
                    .await
                    .expect("remove_paper task panicked");
                let _ = tx.send(Message::Job(id, JobOutcome::Removed { citation_key, result }));
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
        network_kind @ (JobKind::SearchAll(_)
        | JobKind::SearchByAuthor(_)
        | JobKind::SearchByDoi(_)
        | JobKind::AddCandidate(_)) => {
            spawn_network(id, network_kind, ctx, tx);
        }
    }
}

/// `pax_core`'s async functions that touch providers aren't `Send` — the
/// `crossref` crate's HTTP client holds an `Rc` internally — so their
/// futures can't run on `tokio::spawn`'s multi-threaded executor. Each such
/// job gets its own OS thread with a small current-thread runtime instead:
/// still fully off the main event loop's task, just not sharing its
/// runtime. Applies to search and `add_candidate` today; `show_reference`
/// needs the same treatment if a future step calls it.
fn spawn_network(id: JobId, kind: JobKind, ctx: PaxCtx, tx: UnboundedSender<Message>) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build network runtime");
        let outcome = runtime.block_on(run_network_job(kind, ctx));
        let _ = tx.send(Message::Job(id, outcome));
    });
}

async fn run_network_job(kind: JobKind, ctx: PaxCtx) -> JobOutcome {
    match kind {
        JobKind::SearchAll(query) => searched(pax_core::search_all(&query, &ctx.config).await, &ctx),
        JobKind::SearchByAuthor(author) => {
            searched(pax_core::search_by_author(&author, &ctx.config).await, &ctx)
        }
        JobKind::SearchByDoi(doi) => searched(pax_core::search_by_doi(&doi, &ctx.config).await, &ctx),
        JobKind::AddCandidate(candidate_id) => {
            let result = pax_core::add_candidate(&candidate_id, &ctx.root, &ctx.config).await;
            JobOutcome::Added(result)
        }
        JobKind::LoadLibrary
        | JobKind::FetchPaper(_)
        | JobKind::ResolveForOpen(_)
        | JobKind::EditPaper { .. }
        | JobKind::RemovePaper(_)
        | JobKind::Sync
        | JobKind::Check
        | JobKind::ExportToFile { .. } => {
            unreachable!("spawn_network is only called with network JobKinds")
        }
    }
}

fn searched(results: SearchResults, ctx: &PaxCtx) -> JobOutcome {
    let known_dois = pax_core::known_dois(&ctx.root);
    JobOutcome::Searched { results, known_dois }
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
