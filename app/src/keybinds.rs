use gdk4::Key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewAction {
    Keep,
    Trash,
    Previous,
    Next,
    Undo,
    JumpToNextUndecided,
    PlayPause,
    ToggleFullscreen,
    BackToSetup,
}

pub fn action_for_key(key: Key) -> Option<ReviewAction> {
    match key {
        Key::f | Key::F => Some(ReviewAction::Keep),
        Key::j | Key::J => Some(ReviewAction::Trash),
        Key::Left => Some(ReviewAction::Previous),
        Key::Right => Some(ReviewAction::Next),
        Key::BackSpace => Some(ReviewAction::Undo),
        Key::Tab | Key::ISO_Left_Tab => Some(ReviewAction::JumpToNextUndecided),
        Key::space => Some(ReviewAction::PlayPause),
        Key::F11 => Some(ReviewAction::ToggleFullscreen),
        Key::Escape => Some(ReviewAction::BackToSetup),
        _ => None,
    }
}
