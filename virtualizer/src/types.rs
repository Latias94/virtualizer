/// Alignment used by scroll-to helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Align {
    /// Align the item start to the viewport start.
    Start,
    /// Center the item in the viewport.
    Center,
    /// Align the item end to the viewport end.
    End,
    /// Choose `Start`/`End` automatically based on visibility.
    Auto,
}

/// Scroll direction derived from the latest scroll offset update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ScrollDirection {
    /// Scrolling towards increasing offsets.
    Forward,
    /// Scrolling towards decreasing offsets.
    Backward,
}

/// A platform-agnostic viewport rect.
///
/// - `main`: size of the scroll axis (height for vertical lists, width for horizontal lists).
/// - `cross`: size of the cross axis (width for vertical lists, height for horizontal lists).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rect {
    pub main: u32,
    pub cross: u32,
}

/// A half-open range of virtual item indexes: `[start_index, end_index)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VirtualRange {
    pub start_index: usize,
    pub end_index: usize, // exclusive
}

impl VirtualRange {
    /// Returns `true` if the range contains no items.
    pub fn is_empty(&self) -> bool {
        self.start_index >= self.end_index
    }

    /// Returns the last index in the range (inclusive), or `None` if the range is empty.
    pub fn end_inclusive(&self) -> Option<usize> {
        if self.is_empty() {
            None
        } else {
            Some(self.end_index.saturating_sub(1))
        }
    }

    /// Converts the half-open range `[start_index, end_index)` into an inclusive range.
    pub fn as_inclusive(&self) -> Option<core::ops::RangeInclusive<usize>> {
        Some(self.start_index..=self.end_inclusive()?)
    }
}

/// A virtual item produced for rendering.
///
/// `start` includes `scroll_margin` and `padding_start`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VirtualItem {
    pub index: usize,
    /// Start offset in the scroll axis (includes `scroll_margin` and `padding_start`).
    pub start: u64,
    /// Size in the scroll axis (excludes `gap`).
    pub size: u32,
}

impl VirtualItem {
    /// Returns `start + size`.
    pub fn end(&self) -> u64 {
        self.start.saturating_add(self.size as u64)
    }
}

/// A virtual item that carries its stable key.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VirtualItemKeyed<K> {
    pub key: K,
    pub index: usize,
    /// Start offset in the scroll axis (includes `scroll_margin` and `padding_start`).
    pub start: u64,
    /// Size in the scroll axis (excludes `gap`).
    pub size: u32,
}

impl<K> VirtualItemKeyed<K> {
    /// Returns `start + size`.
    pub fn end(&self) -> u64 {
        self.start.saturating_add(self.size as u64)
    }
}

/// Default item key type (index-based).
pub type ItemKey = u64;

/// The input range passed to a [`crate::RangeExtractor`].
///
/// `start_index..end_index` is the visible range (without overscan). `overscan` is provided so
/// extractors can implement pinned/sticky logic while still using the virtualizer's overscan
/// budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Range {
    pub start_index: usize,
    pub end_index: usize, // exclusive, visible range (no overscan)
    pub overscan: usize,
    pub count: usize,
}
