use std::io::{self, Stdout};

use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

/// Owns the raw-mode/alternate-screen terminal state for the whole run.
/// Restores the terminal on drop, so a normal exit or a propagated error
/// both leave the user's shell in a sane state.
pub struct TerminalGuard {
    pub terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    pub fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self { terminal })
    }

    /// Hands the terminal back to a foreground subprocess (the PDF viewer):
    /// leaves raw mode and the alternate screen so the process sees a normal
    /// terminal. Pair with `resume()` once it exits.
    pub fn suspend(&mut self) -> io::Result<()> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }

    /// Reclaims the terminal after `suspend()`. Forces a full repaint on the
    /// next draw, since the screen's contents are stale after the
    /// subprocess wrote its own output over them.
    ///
    /// Deliberately uses `resize()` rather than `Terminal::clear()`:
    /// `clear()` saves and restores the cursor position via a DSR query
    /// (`ESC[6n`) round-tripped through the terminal, which only resolves
    /// if something is actively answering it — some pty layers never do,
    /// hanging/erroring the read. `resize()` on a fullscreen viewport does
    /// the same full clear-and-reset without needing cursor position at
    /// all (confirmed in ratatui's own source: the fullscreen branch never
    /// reads it), so it's strictly the safer choice here even though we're
    /// not actually changing size.
    pub fn resume(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        execute!(self.terminal.backend_mut(), EnterAlternateScreen)?;
        let area = self.terminal.size()?;
        self.terminal.resize(area.into())?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
    }
}
