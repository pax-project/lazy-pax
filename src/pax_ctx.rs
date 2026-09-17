use std::path::PathBuf;

/// Small, cheaply-`Clone`-able bundle threaded into every background job —
/// currently just the library root (the directory containing `research/`).
/// Gains a `pax_core::Config` field once a job needs provider calls
/// (`search_all` and friends, build-order step 4).
#[derive(Clone)]
pub struct PaxCtx {
    pub root: PathBuf,
}

impl PaxCtx {
    pub fn from_env(root: PathBuf) -> Self {
        Self { root }
    }
}
