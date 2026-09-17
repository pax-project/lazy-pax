/// Everything the main loop can receive: terminal input or a timer tick.
/// Background-job completions and viewer-exit results join this enum in
/// later build-order steps, once jobs exist.
pub enum Message {
    Input(crossterm::event::Event),
    Tick,
}

/// Everything `App::update` can ask the main loop to do on its behalf.
pub enum Effect {
    Quit,
}
