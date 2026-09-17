use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::{InsertTarget, Mode, PendingInput};

/// Pure key -> intent mapping. No I/O, no `App` access beyond the small
/// `PendingInput` accumulator (for `gg`) — unit-testable in isolation.
/// Gains a `Screen`/confirm-overlay dispatch once those exist (see
/// docs/status.md's build order).
pub fn map_key(mode: &Mode, pending: &mut PendingInput, key: KeyEvent) -> Option<Action> {
    match mode {
        Mode::Insert(_) => match key.code {
            KeyCode::Esc => Some(Action::CancelInput),
            KeyCode::Enter => Some(Action::SubmitInput),
            KeyCode::Backspace => Some(Action::InputBackspace),
            KeyCode::Char(c) => Some(Action::InputChar(c)),
            _ => None,
        },
        Mode::Normal => normal_mode_key(pending, key),
    }
}

fn normal_mode_key(pending: &mut PendingInput, key: KeyEvent) -> Option<Action> {
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
        KeyCode::Char('/') => Some(Action::EnterInsert(InsertTarget::LibraryFilter)),
        KeyCode::Esc => Some(Action::ClearFilter),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::from(KeyCode::Char(c))
    }

    fn key_code(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    #[test]
    fn q_maps_to_quit() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Mode::Normal, &mut pending, key('q')), Some(Action::Quit));
    }

    #[test]
    fn unmapped_key_is_none() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Mode::Normal, &mut pending, key('z')), None);
    }

    #[test]
    fn j_and_k_map_to_move() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Mode::Normal, &mut pending, key('j')), Some(Action::MoveDown));
        assert_eq!(map_key(&Mode::Normal, &mut pending, key('k')), Some(Action::MoveUp));
    }

    #[test]
    fn capital_g_maps_to_go_bottom() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Mode::Normal, &mut pending, key('G')), Some(Action::GoBottom));
    }

    #[test]
    fn gg_maps_to_go_top() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Mode::Normal, &mut pending, key('g')), None);
        assert!(pending.g_pressed);
        assert_eq!(map_key(&Mode::Normal, &mut pending, key('g')), Some(Action::GoTop));
        assert!(!pending.g_pressed);
    }

    #[test]
    fn g_then_unrelated_key_clears_pending_without_an_action() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Mode::Normal, &mut pending, key('g')), None);
        assert_eq!(map_key(&Mode::Normal, &mut pending, key('j')), None);
        assert!(!pending.g_pressed);
    }

    #[test]
    fn slash_enters_library_filter_insert_mode() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Mode::Normal, &mut pending, key('/')),
            Some(Action::EnterInsert(InsertTarget::LibraryFilter))
        );
    }

    #[test]
    fn esc_in_normal_mode_clears_filter() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Mode::Normal, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::ClearFilter)
        );
    }

    #[test]
    fn insert_mode_types_characters_and_backspace() {
        let mode = Mode::Insert(InsertTarget::LibraryFilter);
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&mode, &mut pending, key('h')), Some(Action::InputChar('h')));
        assert_eq!(
            map_key(&mode, &mut pending, key_code(KeyCode::Backspace)),
            Some(Action::InputBackspace)
        );
    }

    #[test]
    fn insert_mode_enter_submits_and_esc_cancels() {
        let mode = Mode::Insert(InsertTarget::LibraryFilter);
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&mode, &mut pending, key_code(KeyCode::Enter)),
            Some(Action::SubmitInput)
        );
        assert_eq!(
            map_key(&mode, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::CancelInput)
        );
    }

    #[test]
    fn insert_mode_does_not_treat_j_as_navigation() {
        let mode = Mode::Insert(InsertTarget::LibraryFilter);
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&mode, &mut pending, key('j')), Some(Action::InputChar('j')));
    }
}
