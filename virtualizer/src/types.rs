#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Align {
    Start,
    Center,
    End,
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ScrollDirection {
    Forward,
    Backward,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rect {
    pub main: u32,
    pub cross: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VirtualRange {
    pub start_index: usize,
    pub end_index: usize, // exclusive
}

impl VirtualRange {
    pub fn is_empty(&self) -> bool {
        self.start_index >= self.end_index
    }
}

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
    pub fn end(&self) -> u64 {
        self.start.saturating_add(self.size as u64)
    }
}

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
    pub fn end(&self) -> u64 {
        self.start.saturating_add(self.size as u64)
    }
}

pub type ItemKey = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Range {
    pub start_index: usize,
    pub end_index: usize, // exclusive, visible range (no overscan)
    pub overscan: usize,
    pub count: usize,
}
