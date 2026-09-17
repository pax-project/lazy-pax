use std::collections::HashMap;
use std::path::Path;

use pax_core::{CandidateId, CandidateWork, PaxError, PaperRef, ProviderError, ProviderId};
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
}

pub enum JobOutcome {
    Library(Result<Vec<pax_core::Paper>, PaxError>),
    Searched {
        results: SearchResults,
        known_dois: std::collections::HashSet<String>,
    },
    Added(Result<PaperRef, PaxError>),
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
        network_kind => spawn_network(id, network_kind, ctx, tx),
    }
}

/// `pax_core`'s async functions that touch providers aren't `Send` — the
/// `crossref` crate's HTTP client holds an `Rc` internally — so their
/// futures can't run on `tokio::spawn`'s multi-threaded executor. Each such
/// job gets its own OS thread with a small current-thread runtime instead:
/// still fully off the main event loop's task, just not sharing its
/// runtime. Applies to search today and `add_candidate` here; any future
/// job awaiting `resolve_candidate`/`show_reference` needs the same
/// treatment.
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
        JobKind::LoadLibrary => unreachable!("spawn_network is only called with network JobKinds"),
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
