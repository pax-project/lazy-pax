use std::path::PathBuf;

use crate::job::{JobId, JobKind, JobOutcome};

/// Everything the main loop can receive: terminal input, a timer tick, a
/// background job completing, or the external PDF viewer exiting.
pub enum Message {
    Input(crossterm::event::Event),
    Tick,
    Job(JobId, JobOutcome),
    ViewerExited(String, std::io::Result<std::process::ExitStatus>),
}

/// Everything `App::update` can ask the main loop to do on its behalf.
pub enum Effect {
    Quit,
    Spawn(JobKind),
    /// Suspend the terminal and hand it to `$PAX_PDF_VIEWER` (default
    /// `xdg-open`) on `path`, then resume and feed the result back as
    /// `Message::ViewerExited`.
    LaunchViewer { citation_key: String, path: PathBuf },
}
