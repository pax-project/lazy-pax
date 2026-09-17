mod action;
mod app;
mod job;
mod keymap;
mod message;
mod pax_ctx;
mod status;
mod terminal;
mod ui;

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, LeaveAlternateScreen};
use tokio::sync::mpsc;

use app::App;
use job::JobId;
use message::{Effect, Message};
use pax_ctx::PaxCtx;
use terminal::TerminalGuard;

/// Restores the terminal before the default panic hook runs, so a panic
/// never strands the user in raw mode / the alternate screen.
fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));
}

/// Blocks on `crossterm::event::read()` forever on its own OS thread;
/// abandoned (not joined) on quit, same as ratatui's own examples.
fn spawn_input_thread(tx: mpsc::UnboundedSender<Message>) {
    std::thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            if tx.send(Message::Input(event)).is_err() {
                break;
            }
        }
    });
}

fn handle_effect(
    effect: Effect,
    ctx: &PaxCtx,
    tx: &mpsc::UnboundedSender<Message>,
    next_job_id: &mut JobId,
    terminal: &mut TerminalGuard,
    app: &mut App,
) -> io::Result<()> {
    match effect {
        Effect::Quit => {}
        Effect::Spawn(kind) => {
            *next_job_id += 1;
            job::spawn(*next_job_id, kind, ctx.clone(), tx.clone());
        }
        Effect::LaunchViewer { citation_key, path } => {
            let viewer = std::env::var("PAX_PDF_VIEWER").unwrap_or_else(|_| "xdg-open".to_string());
            terminal.suspend()?;
            let result = std::process::Command::new(&viewer).arg(&path).status();
            terminal.resume()?;
            // Fed back into update() directly, in the same tick — this
            // already happened synchronously, no need to round-trip it
            // through the channel like a background job's result.
            for effect in app.update(Message::ViewerExited(citation_key, result)) {
                handle_effect(effect, ctx, tx, next_job_id, terminal, app)?;
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    install_panic_hook();
    let mut terminal_guard = TerminalGuard::new()?;

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    spawn_input_thread(tx.clone());

    let ctx = PaxCtx::from_env(PathBuf::from("."));
    let mut next_job_id: JobId = 0;
    let mut app = App::new();

    for effect in app.init_effects() {
        handle_effect(effect, &ctx, &tx, &mut next_job_id, &mut terminal_guard, &mut app)?;
    }

    let mut tick = tokio::time::interval(Duration::from_millis(100));

    loop {
        let msg = tokio::select! {
            Some(msg) = rx.recv() => msg,
            _ = tick.tick() => Message::Tick,
        };

        for effect in app.update(msg) {
            handle_effect(effect, &ctx, &tx, &mut next_job_id, &mut terminal_guard, &mut app)?;
        }

        terminal_guard.terminal.draw(|frame| ui::draw(frame, &app))?;

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
