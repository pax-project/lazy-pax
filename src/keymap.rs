use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::PendingInput;

/// Pure key -> intent mapping. No I/O, no `App` access beyond the small
/// `PendingInput` accumulator (for `gg`) — unit-testable in isolation.
/// Gains a `Screen`/`Mode`/confirm-overlay dispatch once those exist (see
/// docs/status.md's build order).
pub fn map_key(pending: &mut PendingInput, key: KeyEvent) -> Option<Action> {
    if pending.g_pressed {
        pending.g_pressed = false;
        return match key.code {
            KeyCode::Char('g') => Some(Action::GoTop),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Char('G') => Some(Action::GoBottom),
        KeyCode::Char('g') => {
            pending.g_pressed = true;
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::from(KeyCode::Char(c))
    }

    #[test]
    fn q_maps_to_quit() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&mut pending, key('q')), Some(Action::Quit));
    }

    #[test]
    fn unmapped_key_is_none() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&mut pending, key('z')), None);
    }

    #[test]
    fn j_and_k_map_to_move() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&mut pending, key('j')), Some(Action::MoveDown));
        assert_eq!(map_key(&mut pending, key('k')), Some(Action::MoveUp));
    }

    #[test]
    fn capital_g_maps_to_go_bottom() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&mut pending, key('G')), Some(Action::GoBottom));
    }

    #[test]
    fn gg_maps_to_go_top() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&mut pending, key('g')), None);
        assert!(pending.g_pressed);
        assert_eq!(map_key(&mut pending, key('g')), Some(Action::GoTop));
        assert!(!pending.g_pressed);
    }

    #[test]
    fn g_then_unrelated_key_clears_pending_without_an_action() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&mut pending, key('g')), None);
        assert_eq!(map_key(&mut pending, key('j')), None);
        assert!(!pending.g_pressed);
    }
}
