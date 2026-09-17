use std::path::Path;

use tokio::sync::mpsc::UnboundedSender;

use crate::message::Message;
use crate::pax_ctx::PaxCtx;

pub type JobId = u64;

/// The only module allowed to call into `pax_core` directly — every other
/// module works with `pax_core` *types* (`Paper`, `PaxError`, ...) but never
/// calls its functions itself. Keeps "never shells out / hand-parses
/// papers.nix" auditable in one place.
pub enum JobKind {
    LoadLibrary,
}

pub enum JobOutcome {
    Library(Result<Vec<pax_core::Paper>, pax_core::PaxError>),
}

pub fn spawn(id: JobId, kind: JobKind, ctx: PaxCtx, tx: UnboundedSender<Message>) {
    tokio::spawn(async move {
        let outcome = match kind {
            JobKind::LoadLibrary => {
                let root = ctx.root.clone();
                let result = tokio::task::spawn_blocking(move || load_library(&root))
                    .await
                    .expect("load_library task panicked");
                JobOutcome::Library(result)
            }
        };
        let _ = tx.send(Message::Job(id, outcome));
    });
}

fn load_library(root: &Path) -> Result<Vec<pax_core::Paper>, pax_core::PaxError> {
    let path = pax_core::nix::papers_path(root);
    let library = pax_core::Library::load(&path)?;
    Ok(library.papers().to_vec())
}
