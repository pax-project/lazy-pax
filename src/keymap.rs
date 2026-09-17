use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;

/// Pure key -> intent mapping. No I/O, no `App` access — unit-testable in
/// isolation. Gains a `Screen`/`Mode`/confirm-overlay/pending-input dispatch
/// once those exist (see docs/status.md's build order).
pub fn map_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q_maps_to_quit() {
        let key = KeyEvent::from(KeyCode::Char('q'));
        assert_eq!(map_key(key), Some(Action::Quit));
    }

    #[test]
    fn unmapped_key_is_none() {
        let key = KeyEvent::from(KeyCode::Char('z'));
        assert_eq!(map_key(key), None);
    }
}
