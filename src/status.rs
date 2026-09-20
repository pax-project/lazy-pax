use std::time::{Duration, Instant};

/// How long a `Success`/`Error` message stays on screen before it's cleared
/// back to the hint text. Without this, a message (and the busy state that
/// often produced it) can sit there indefinitely, permanently hiding the
/// command hints underneath it.
const MESSAGE_TTL: Duration = Duration::from_secs(5);

/// The status/error line shown at the bottom of every screen — the single
/// funnel every `JobOutcome` failure/success is meant to pass through
/// (see docs' "no silent failure or crash" requirement).
#[derive(Default)]
pub struct StatusBar {
    pub message: Option<(StatusKind, String)>,
    /// When `message` was last set — drives `clear_if_expired`. `None`
    /// whenever `message` is `None`.
    set_at: Option<Instant>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Error,
    Success,
    /// A job that can take real, visible time (fetch/open, which shell out
    /// to `nix`) is in flight — shown from the moment it's dispatched, not
    /// just once it resolves, since the DoD calls for "visible pending
    /// state" during the wait itself. Never auto-expires: it's replaced by
    /// the job's own outcome, not by a timer, since expiring it early would
    /// hide a job that's still genuinely running.
    Pending,
}

impl StatusBar {
    pub fn error(&mut self, msg: impl Into<String>) {
        self.message = Some((StatusKind::Error, msg.into()));
        self.set_at = Some(Instant::now());
    }

    pub fn success(&mut self, msg: impl Into<String>) {
        self.message = Some((StatusKind::Success, msg.into()));
        self.set_at = Some(Instant::now());
    }

    pub fn pending(&mut self, msg: impl Into<String>) {
        self.message = Some((StatusKind::Pending, msg.into()));
        self.set_at = Some(Instant::now());
    }

    /// Clears `message` once it's a `Success`/`Error` that's been showing
    /// for at least `MESSAGE_TTL` — called on every `Message::Tick` so the
    /// command hints underneath reappear on their own instead of staying
    /// hidden until the next status update happens to overwrite it.
    pub fn clear_if_expired(&mut self) {
        let expired = matches!(
            (&self.message, self.set_at),
            (Some((kind, _)), Some(set_at))
                if *kind != StatusKind::Pending && set_at.elapsed() >= MESSAGE_TTL
        );
        if expired {
            self.message = None;
            self.set_at = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_message_is_not_cleared() {
        let mut status = StatusBar::default();
        status.success("done");
        status.clear_if_expired();
        assert!(status.message.is_some());
    }

    #[test]
    fn a_success_message_expires_after_the_ttl() {
        let mut status = StatusBar::default();
        status.success("done");
        status.set_at = Some(Instant::now() - MESSAGE_TTL);
        status.clear_if_expired();
        assert!(status.message.is_none());
    }

    #[test]
    fn an_error_message_expires_after_the_ttl() {
        let mut status = StatusBar::default();
        status.error("failed");
        status.set_at = Some(Instant::now() - MESSAGE_TTL);
        status.clear_if_expired();
        assert!(status.message.is_none());
    }

    #[test]
    fn pending_never_auto_expires() {
        let mut status = StatusBar::default();
        status.pending("working…");
        status.set_at = Some(Instant::now() - Duration::from_secs(3600));
        status.clear_if_expired();
        assert!(status.message.is_some(), "a still-running job's status must not be hidden by a timer");
    }

    #[test]
    fn clearing_an_already_empty_bar_is_a_no_op() {
        let mut status = StatusBar::default();
        status.clear_if_expired();
        assert!(status.message.is_none());
    }
}
