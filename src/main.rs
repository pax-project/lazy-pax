mod action;
mod app;
mod keymap;
mod message;
mod terminal;
mod ui;

use std::io;
use std::time::Duration;

use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, LeaveAlternateScreen};
use tokio::sync::mpsc;

use app::App;
use message::{Effect, Message};
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

#[tokio::main]
async fn main() -> io::Result<()> {
    install_panic_hook();
    let mut terminal_guard = TerminalGuard::new()?;

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    spawn_input_thread(tx);

    let mut app = App::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));

    loop {
        let msg = tokio::select! {
            Some(msg) = rx.recv() => msg,
            _ = tick.tick() => Message::Tick,
        };

        for effect in app.update(msg) {
            match effect {
                Effect::Quit => {}
            }
        }

        terminal_guard.terminal.draw(|frame| ui::draw(frame, &app))?;

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
