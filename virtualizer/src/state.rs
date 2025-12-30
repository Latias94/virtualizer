use crate::Rect;

/// A lightweight, serializable snapshot of the current viewport geometry.
///
/// With `feature = "serde"`, this type implements `Serialize`/`Deserialize`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ViewportState {
    pub rect: Rect,
}

/// A lightweight, serializable snapshot of the current scroll state.
///
/// With `feature = "serde"`, this type implements `Serialize`/`Deserialize`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScrollState {
    pub offset: u64,
    pub is_scrolling: bool,
}

/// A combined snapshot of viewport + scroll state.
///
/// This is useful for restoring UI state across frames or sessions without
/// coupling the virtualizer to any specific UI framework.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FrameState {
    pub viewport: ViewportState,
    pub scroll: ScrollState,
}
