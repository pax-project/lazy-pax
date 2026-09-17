use std::collections::HashMap;
use std::path::Path;

use pax_core::{CandidateWork, ProviderError, ProviderId};
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
}

pub enum JobOutcome {
    Library(Result<Vec<pax_core::Paper>, pax_core::PaxError>),
    Searched {
        results: SearchResults,
        known_dois: std::collections::HashSet<String>,
    },
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
        search_kind @ (JobKind::SearchAll(_) | JobKind::SearchByAuthor(_) | JobKind::SearchByDoi(_)) => {
            spawn_search(id, search_kind, ctx, tx);
        }
    }
}

/// `pax_core`'s search functions aren't `Send` — the `crossref` crate's HTTP
/// client holds an `Rc` internally — so their futures can't run on
/// `tokio::spawn`'s multi-threaded executor. Each search gets its own OS
/// thread with a small current-thread runtime instead: still fully off the
/// main event loop's task, just not sharing its runtime.
fn spawn_search(id: JobId, kind: JobKind, ctx: PaxCtx, tx: UnboundedSender<Message>) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build search runtime");
        let outcome = runtime.block_on(async {
            let results: SearchResults = match kind {
                JobKind::SearchAll(query) => pax_core::search_all(&query, &ctx.config).await,
                JobKind::SearchByAuthor(author) => pax_core::search_by_author(&author, &ctx.config).await,
                JobKind::SearchByDoi(doi) => pax_core::search_by_doi(&doi, &ctx.config).await,
                JobKind::LoadLibrary => unreachable!("spawn_search is only called with search JobKinds"),
            };
            let known_dois = pax_core::known_dois(&ctx.root);
            JobOutcome::Searched { results, known_dois }
        });
        let _ = tx.send(Message::Job(id, outcome));
    });
}

fn load_library(root: &Path) -> Result<Vec<pax_core::Paper>, pax_core::PaxError> {
    let path = pax_core::nix::papers_path(root);
    let library = pax_core::Library::load(&path)?;
    Ok(library.papers().to_vec())
}
