use crate::job::{JobId, JobKind, JobOutcome};

/// Everything the main loop can receive: terminal input, a timer tick, or a
/// background job completing. Viewer-exit results join this enum once the
/// open flow exists.
pub enum Message {
    Input(crossterm::event::Event),
    Tick,
    Job(JobId, JobOutcome),
}

/// Everything `App::update` can ask the main loop to do on its behalf.
pub enum Effect {
    Quit,
    Spawn(JobKind),
}
