/// The status/error line shown at the bottom of every screen — the single
/// funnel every `JobOutcome` failure/success is meant to pass through
/// (see docs' "no silent failure or crash" requirement).
#[derive(Default)]
pub struct StatusBar {
    pub message: Option<(StatusKind, String)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Error,
    Success,
}

impl StatusBar {
    pub fn error(&mut self, msg: impl Into<String>) {
        self.message = Some((StatusKind::Error, msg.into()));
    }

    pub fn success(&mut self, msg: impl Into<String>) {
        self.message = Some((StatusKind::Success, msg.into()));
    }
}
