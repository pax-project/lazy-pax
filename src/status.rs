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
    /// A job that can take real, visible time (fetch/open, which shell out
    /// to `nix`) is in flight — shown from the moment it's dispatched, not
    /// just once it resolves, since the DoD calls for "visible pending
    /// state" during the wait itself.
    Pending,
}

impl StatusBar {
    pub fn error(&mut self, msg: impl Into<String>) {
        self.message = Some((StatusKind::Error, msg.into()));
    }

    pub fn success(&mut self, msg: impl Into<String>) {
        self.message = Some((StatusKind::Success, msg.into()));
    }

    pub fn pending(&mut self, msg: impl Into<String>) {
        self.message = Some((StatusKind::Pending, msg.into()));
    }
}
