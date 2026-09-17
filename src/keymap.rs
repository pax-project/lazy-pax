use crossterm::event::{KeyCode, KeyEvent};

use crate::action::Action;
use crate::app::{InsertTarget, Mode, PendingInput, Screen};

/// Pure key -> intent mapping. No I/O, no `App` access beyond the small
/// `PendingInput` accumulator (for `gg`) — unit-testable in isolation.
/// `confirm_active` is checked before anything else: while a confirmation
/// overlay is up, no other key (including navigation) does anything but
/// answer it — this is what makes it safe for `Action::ConfirmYes` to act
/// on "whatever's currently selected" without re-resolving the target.
pub fn map_key(
    screen: &Screen,
    mode: &Mode,
    confirm_active: bool,
    pending: &mut PendingInput,
    key: KeyEvent,
) -> Option<Action> {
    if confirm_active {
        return match key.code {
            KeyCode::Char('y') | KeyCode::Enter => Some(Action::ConfirmYes),
            KeyCode::Char('n') | KeyCode::Esc => Some(Action::ConfirmNo),
            _ => None,
        };
    }
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
            Screen::Search | Screen::Detail | Screen::Edit | Screen::SyncReport | Screen::CheckReport => Action::Back,
        }),
        KeyCode::Char('/') if matches!(screen, Screen::Library) => {
            Some(Action::EnterInsert(InsertTarget::LibraryFilter))
        }
        KeyCode::Char('/') if matches!(screen, Screen::Search) => {
            Some(Action::EnterInsert(InsertTarget::SearchQuery))
        }
        KeyCode::Char('S') if matches!(screen, Screen::Library) => Some(Action::GoToSearch),
        KeyCode::Tab if matches!(screen, Screen::Search) => Some(Action::CycleSearchMode),
        KeyCode::Enter | KeyCode::Char('l') if matches!(screen, Screen::Search | Screen::Library) => {
            Some(Action::OpenDetail)
        }
        KeyCode::Char('a') if matches!(screen, Screen::Detail) => Some(Action::AddCandidate),
        KeyCode::Char('f') if matches!(screen, Screen::Library | Screen::Detail) => Some(Action::Fetch),
        KeyCode::Char('o') if matches!(screen, Screen::Library | Screen::Detail) => Some(Action::Open),
        KeyCode::Char('e') if matches!(screen, Screen::Library | Screen::Detail) => Some(Action::EnterEdit),
        KeyCode::Char('d') if matches!(screen, Screen::Library | Screen::Detail) => Some(Action::TriggerRemove),
        KeyCode::Char('s') if matches!(screen, Screen::Library) => Some(Action::TriggerSync),
        KeyCode::Char('c') if matches!(screen, Screen::Library) => Some(Action::TriggerCheck),
        KeyCode::Char('w') if matches!(screen, Screen::Edit) => Some(Action::SaveEdit),
        KeyCode::Char('a') if matches!(screen, Screen::Edit) => Some(Action::EnterInsert(InsertTarget::EditTagAdd)),
        KeyCode::Char('x') if matches!(screen, Screen::Edit) => Some(Action::RemoveLastTag),
        KeyCode::Char('i') | KeyCode::Enter if matches!(screen, Screen::Edit) => Some(Action::EditFocusedField),
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
            map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('q')),
            Some(Action::Quit)
        );
    }

    #[test]
    fn unmapped_key_is_none() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('z')), None);
    }

    #[test]
    fn j_and_k_map_to_move() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('j')),
            Some(Action::MoveDown)
        );
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('k')),
            Some(Action::MoveUp)
        );
    }

    #[test]
    fn capital_g_maps_to_go_bottom() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('G')),
            Some(Action::GoBottom)
        );
    }

    #[test]
    fn gg_maps_to_go_top() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('g')), None);
        assert!(pending.g_pressed);
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('g')),
            Some(Action::GoTop)
        );
        assert!(!pending.g_pressed);
    }

    #[test]
    fn g_then_unrelated_key_clears_pending_without_an_action() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('g')), None);
        assert_eq!(map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('j')), None);
        assert!(!pending.g_pressed);
    }

    #[test]
    fn slash_enters_library_filter_insert_mode_on_library_screen() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('/')),
            Some(Action::EnterInsert(InsertTarget::LibraryFilter))
        );
    }

    #[test]
    fn slash_enters_search_query_insert_mode_on_search_screen() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Search, &Mode::Normal, false, &mut pending, key('/')),
            Some(Action::EnterInsert(InsertTarget::SearchQuery))
        );
    }

    #[test]
    fn esc_in_normal_mode_clears_filter_on_library_and_goes_back_on_search_or_detail() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::ClearFilter)
        );
        assert_eq!(
            map_key(&Screen::Search, &Mode::Normal, false, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::Back)
        );
        assert_eq!(
            map_key(&Screen::Detail, &Mode::Normal, false, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::Back)
        );
    }

    #[test]
    fn enter_and_l_open_detail_on_search_and_library_but_not_detail() {
        let mut pending = PendingInput::default();
        for screen in [Screen::Search, Screen::Library] {
            assert_eq!(
                map_key(&screen, &Mode::Normal, false, &mut pending, key_code(KeyCode::Enter)),
                Some(Action::OpenDetail)
            );
            assert_eq!(
                map_key(&screen, &Mode::Normal, false, &mut pending, key('l')),
                Some(Action::OpenDetail)
            );
        }
        assert_eq!(
            map_key(&Screen::Detail, &Mode::Normal, false, &mut pending, key_code(KeyCode::Enter)),
            None
        );
    }

    #[test]
    fn capital_s_goes_to_search_only_from_library() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('S')),
            Some(Action::GoToSearch)
        );
        assert_eq!(map_key(&Screen::Search, &Mode::Normal, false, &mut pending, key('S')), None);
    }

    #[test]
    fn tab_cycles_search_mode_only_on_search_screen() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Search, &Mode::Normal, false, &mut pending, key_code(KeyCode::Tab)),
            Some(Action::CycleSearchMode)
        );
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key_code(KeyCode::Tab)),
            None
        );
    }

    #[test]
    fn tab_cycles_search_mode_while_editing_the_query_too() {
        let mode = Mode::Insert(InsertTarget::SearchQuery);
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Search, &mode, false, &mut pending, key_code(KeyCode::Tab)),
            Some(Action::CycleSearchMode)
        );
    }

    #[test]
    fn a_adds_the_candidate_only_on_detail_screen() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Detail, &Mode::Normal, false, &mut pending, key('a')),
            Some(Action::AddCandidate)
        );
        assert_eq!(map_key(&Screen::Search, &Mode::Normal, false, &mut pending, key('a')), None);
    }

    #[test]
    fn f_and_o_fetch_and_open_on_library_and_detail_but_not_search() {
        let mut pending = PendingInput::default();
        for screen in [Screen::Library, Screen::Detail] {
            assert_eq!(map_key(&screen, &Mode::Normal, false, &mut pending, key('f')), Some(Action::Fetch));
            assert_eq!(map_key(&screen, &Mode::Normal, false, &mut pending, key('o')), Some(Action::Open));
        }
        assert_eq!(map_key(&Screen::Search, &Mode::Normal, false, &mut pending, key('f')), None);
        assert_eq!(map_key(&Screen::Search, &Mode::Normal, false, &mut pending, key('o')), None);
    }

    #[test]
    fn e_enters_edit_from_library_and_detail_but_not_search() {
        let mut pending = PendingInput::default();
        for screen in [Screen::Library, Screen::Detail] {
            assert_eq!(map_key(&screen, &Mode::Normal, false, &mut pending, key('e')), Some(Action::EnterEdit));
        }
        assert_eq!(map_key(&Screen::Search, &Mode::Normal, false, &mut pending, key('e')), None);
    }

    #[test]
    fn edit_screen_bindings() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Screen::Edit, &Mode::Normal, false, &mut pending, key('w')), Some(Action::SaveEdit));
        assert_eq!(
            map_key(&Screen::Edit, &Mode::Normal, false, &mut pending, key('a')),
            Some(Action::EnterInsert(InsertTarget::EditTagAdd))
        );
        assert_eq!(
            map_key(&Screen::Edit, &Mode::Normal, false, &mut pending, key('x')),
            Some(Action::RemoveLastTag)
        );
        assert_eq!(
            map_key(&Screen::Edit, &Mode::Normal, false, &mut pending, key('i')),
            Some(Action::EditFocusedField)
        );
        assert_eq!(
            map_key(&Screen::Edit, &Mode::Normal, false, &mut pending, key_code(KeyCode::Enter)),
            Some(Action::EditFocusedField)
        );
        assert_eq!(
            map_key(&Screen::Edit, &Mode::Normal, false, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::Back)
        );
    }

    #[test]
    fn edit_only_bindings_are_screen_scoped() {
        let mut pending = PendingInput::default();
        assert_eq!(map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('w')), None);
        assert_eq!(map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('x')), None);
    }

    #[test]
    fn d_triggers_remove_on_library_and_detail_but_not_search() {
        let mut pending = PendingInput::default();
        for screen in [Screen::Library, Screen::Detail] {
            assert_eq!(
                map_key(&screen, &Mode::Normal, false, &mut pending, key('d')),
                Some(Action::TriggerRemove)
            );
        }
        assert_eq!(map_key(&Screen::Search, &Mode::Normal, false, &mut pending, key('d')), None);
    }

    #[test]
    fn s_and_c_trigger_sync_and_check_only_from_library() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('s')),
            Some(Action::TriggerSync)
        );
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, false, &mut pending, key('c')),
            Some(Action::TriggerCheck)
        );
        assert_eq!(map_key(&Screen::Detail, &Mode::Normal, false, &mut pending, key('s')), None);
        assert_eq!(map_key(&Screen::Search, &Mode::Normal, false, &mut pending, key('c')), None);
    }

    #[test]
    fn esc_goes_back_from_sync_and_check_report_screens() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::SyncReport, &Mode::Normal, false, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::Back)
        );
        assert_eq!(
            map_key(&Screen::CheckReport, &Mode::Normal, false, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::Back)
        );
    }

    #[test]
    fn confirm_active_intercepts_every_key_before_anything_else() {
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, true, &mut pending, key('y')),
            Some(Action::ConfirmYes)
        );
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, true, &mut pending, key_code(KeyCode::Enter)),
            Some(Action::ConfirmYes)
        );
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, true, &mut pending, key('n')),
            Some(Action::ConfirmNo)
        );
        assert_eq!(
            map_key(&Screen::Library, &Mode::Normal, true, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::ConfirmNo)
        );
        // Navigation and every other binding are swallowed while confirming.
        assert_eq!(map_key(&Screen::Library, &Mode::Normal, true, &mut pending, key('j')), None);
        assert_eq!(map_key(&Screen::Library, &Mode::Normal, true, &mut pending, key('q')), None);
        // Even in Insert mode — confirm always wins.
        let insert = Mode::Insert(InsertTarget::LibraryFilter);
        assert_eq!(
            map_key(&Screen::Library, &insert, true, &mut pending, key('y')),
            Some(Action::ConfirmYes)
        );
    }

    #[test]
    fn insert_mode_types_characters_and_backspace() {
        let mode = Mode::Insert(InsertTarget::LibraryFilter);
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &mode, false, &mut pending, key('h')),
            Some(Action::InputChar('h'))
        );
        assert_eq!(
            map_key(&Screen::Library, &mode, false, &mut pending, key_code(KeyCode::Backspace)),
            Some(Action::InputBackspace)
        );
    }

    #[test]
    fn insert_mode_enter_submits_and_esc_cancels() {
        let mode = Mode::Insert(InsertTarget::LibraryFilter);
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &mode, false, &mut pending, key_code(KeyCode::Enter)),
            Some(Action::SubmitInput)
        );
        assert_eq!(
            map_key(&Screen::Library, &mode, false, &mut pending, key_code(KeyCode::Esc)),
            Some(Action::CancelInput)
        );
    }

    #[test]
    fn insert_mode_does_not_treat_j_as_navigation() {
        let mode = Mode::Insert(InsertTarget::LibraryFilter);
        let mut pending = PendingInput::default();
        assert_eq!(
            map_key(&Screen::Library, &mode, false, &mut pending, key('j')),
            Some(Action::InputChar('j'))
        );
    }
}
