use std::path::PathBuf;

/// Small, cheaply-`Clone`-able bundle threaded into every background job —
/// the library root (the directory containing `research/`) and the
/// `pax_core::Config` needed for provider calls. Built once at startup from
/// the environment, matching how the `pax` CLI itself reads it in
/// `src/bin/pax/main.rs`.
#[derive(Clone)]
pub struct PaxCtx {
    pub root: PathBuf,
    pub config: pax_core::Config,
}

impl PaxCtx {
    pub fn from_env(root: PathBuf) -> Self {
        Self {
            root,
            config: pax_core::Config {
                semantic_scholar_api_key: std::env::var("SEMANTIC_SCHOLAR_API_KEY").ok(),
                arxiv_contact: std::env::var("ARXIV_CONTACT").ok(),
            },
        }
    }
}
