use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::{InsertTarget, Mode, PendingInput, Screen};

/// Pure key -> intent mapping. No I/O, no `App` access beyond the small
/// `PendingInput` accumulator (for `gg`) — unit-testable in isolation.
pub fn map_key(screen: &Screen, mode: &Mode, pending: &mut PendingInput, key: KeyEvent) -> Option<Action> {
    match mode {
        Mode::Insert(_) => match key.code {
            KeyCode::Esc => Some(Action::CancelInput),
            KeyCode::Enter => Some(Action::SubmitInput),
            KeyCode::Backspace => Some(Action::InputBackspace),
            KeyCode::Tab if matches!(screen, Screen::Search) => Some(Action::CycleSearchMode),
            KeyCode::Char(c) => Some(Action::InputChar(c)),
            _ => None,
        },
        Mode::Normal => normal_mode_key(screen, pending, key),
    }
}

fn normal_mode_key(screen: &Screen, pending: &mut PendingInput, key: KeyEvent) -> Option<Action> {
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
        KeyCode::Esc => Some(match screen {
            Screen::Library => Action::ClearFilter,
            Screen::Search | Screen::Detail => Action::Back,
        }),
        KeyCode::Char('/') if matches!(screen, Screen::Library) => {
            Some(Action::EnterInsert(InsertTarget::LibraryFilter))
        }
        KeyCode::Char('/') if matches!(screen, Screen::Search) => {
            Some(Action::EnterInsert(InsertTarget::SearchQuery))
        }
        KeyCode::Char('S') if matches!(screen, Screen::Library) => Some(Action::GoToSearch),
        KeyCode::Tab if matches!(screen, Screen::Search) => Some(Action::CycleSearchMode),
        KeyCode::Enter | KeyCode::Char('l') if matches!(screen, Screen::Search) => Some(Action::OpenDetail),
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
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, &mut pending, key('q')),
            Some(Action::Quit)
        );
    }

    #[test]
    fn unmapped_key_is_none() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Screen::Library, &Mode::Normal, &mut pending, key('z')), None);
    }

    #[test]
    fn j_and_k_map_to_move() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, &mut pending, key('j')),
            Some(Action::MoveDown)
        );
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, &mut pending, key('k')),
            Some(Action::MoveUp)
        );
    }

    #[test]
    fn capital_g_maps_to_go_bottom() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, &mut pending, key('G')),
            Some(Action::GoBottom)
        );
    }

    #[test]
    fn gg_maps_to_go_top() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Screen::Library, &Mode::Normal, &mut pending, key('g')), None);
        assert!(pending.g_pressed);
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, &mut pending, key('g')),
            Some(Action::GoTop)
        );
        assert!(!pending.g_pressed);
    }

    #[test]
    fn g_then_unrelated_key_clears_pending_without_an_action() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Screen::Library, &Mode::Normal, &mut pending, key('g')), None);
        assert_eq!(map_key(&Screen::Library, &Mode::Normal, &mut pending, key('j')), None);
        assert!(!pending.g_pressed);
    }

    #[test]
    fn slash_enters_library_filter_insert_mode_on_library_screen() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, &mut pending, key('/')),
            Some(Action::EnterInsert(InsertTarget::LibraryFilter))
        );
    }

    #[test]
    fn slash_enters_search_query_insert_mode_on_search_screen() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Search, &Mode::Normal, &mut pending, key('/')),
            Some(Action::EnterInsert(InsertTarget::SearchQuery))
        );
    }

    #[test]
    fn esc_in_normal_mode_clears_filter_on_library_and_goes_back_on_search_or_detail() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::ClearFilter)
        );
        assert_eq!(
            map_key(&Screen::Search, &Mode::Normal, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::Back)
        );
        assert_eq!(
            map_key(&Screen::Detail, &Mode::Normal, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::Back)
        );
    }

    #[test]
    fn enter_and_l_open_detail_on_search_screen_only() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Search, &Mode::Normal, &mut pending, key_code(KeyCode::Enter)),
            Some(Action::OpenDetail)
        );
        assert_eq!(
            map_key(&Screen::Search, &Mode::Normal, &mut pending, key('l')),
            Some(Action::OpenDetail)
        );
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, &mut pending, key_code(KeyCode::Enter)),
            None
        );
    }

    #[test]
    fn capital_s_goes_to_search_only_from_library() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, &mut pending, key('S')),
            Some(Action::GoToSearch)
        );
        assert_eq!(map_key(&Screen::Search, &Mode::Normal, &mut pending, key('S')), None);
    }

    #[test]
    fn tab_cycles_search_mode_only_on_search_screen() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Search, &Mode::Normal, &mut pending, key_code(KeyCode::Tab)),
            Some(Action::CycleSearchMode)
        );
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, &mut pending, key_code(KeyCode::Tab)),
            None
        );
    }

    #[test]
    fn tab_cycles_search_mode_while_editing_the_query_too() {
        let mode = Mode::Insert(InsertTarget::SearchQuery);
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Search, &mode, &mut pending, key_code(KeyCode::Tab)),
            Some(Action::CycleSearchMode)
        );
    }

    #[test]
    fn insert_mode_types_characters_and_backspace() {
        let mode = Mode::Insert(InsertTarget::LibraryFilter);
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &mode, &mut pending, key('h')),
            Some(Action::InputChar('h'))
        );
        assert_eq!(
            map_key(&Screen::Library, &mode, &mut pending, key_code(KeyCode::Backspace)),
            Some(Action::InputBackspace)
        );
    }

    #[test]
    fn insert_mode_enter_submits_and_esc_cancels() {
        let mode = Mode::Insert(InsertTarget::LibraryFilter);
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &mode, &mut pending, key_code(KeyCode::Enter)),
            Some(Action::SubmitInput)
        );
        assert_eq!(
            map_key(&Screen::Library, &mode, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::CancelInput)
        );
    }

    #[test]
    fn insert_mode_does_not_treat_j_as_navigation() {
        let mode = Mode::Insert(InsertTarget::LibraryFilter);
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &mode, &mut pending, key('j')),
            Some(Action::InputChar('j'))
        );
    }
}
