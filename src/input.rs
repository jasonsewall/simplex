/// Tracks live mouse state across events.
#[derive(Default)]
pub struct InputState {
    /// Current cursor position in physical pixels.
    pub cursor: (f32, f32),
    pub lmb_down: bool,
    pub rmb_down: bool,
}
