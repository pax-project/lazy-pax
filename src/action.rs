use crate::app::InsertTarget;

/// Screen-agnostic user intents produced by `keymap`. Grows one variant at a
/// time as each screen/feature is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    MoveDown,
    MoveUp,
    GoTop,
    GoBottom,
    EnterInsert(InsertTarget),
    CancelInput,
    SubmitInput,
    InputChar(char),
    InputBackspace,
    ClearFilter,
    GoToSearch,
    Back,
    CycleSearchMode,
    OpenDetail,
}
