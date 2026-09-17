/// The status/error line shown at the bottom of every screen — the single
/// funnel every `JobOutcome` failure/success is meant to pass through
/// (see docs' "no silent failure or crash" requirement).
#[derive(Default)]
pub struct StatusBar {
    pub message: Option<String>,
}

impl StatusBar {
    pub fn error(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
    }
}
