use core::fmt;

use crate::VirtualizerKey;

/// A scroll anchor that can be used to preserve visual position across data changes.
///
/// Typical use cases:
/// - chat/timeline "prepend" (load older messages above) without content jumping
/// - any reorder/replace where you want the viewport to stay anchored to an item identity
#[derive(Clone, PartialEq, Eq)]
pub struct ScrollAnchor<K> {
    pub key: K,
    /// The distance from the anchor item's start to the viewport's scroll offset.
    pub offset_in_viewport: u64,
}

impl<K: fmt::Debug> fmt::Debug for ScrollAnchor<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScrollAnchor")
            .field("key", &self.key)
            .field("offset_in_viewport", &self.offset_in_viewport)
            .finish()
    }
}

/// Captures an anchor for the first visible item (by key).
///
/// Returns `None` if the virtualizer is disabled or the visible range is empty.
pub fn capture_first_visible_anchor<K: VirtualizerKey>(
    v: &virtualizer::Virtualizer<K>,
) -> Option<ScrollAnchor<K>> {
    let visible = v.visible_range();
    if visible.is_empty() {
        return None;
    }
    let index = visible.start_index;
    let start = v.item_start(index)?;
    let key = v.key_for(index);
    let offset_in_viewport = v.scroll_offset().saturating_sub(start);
    Some(ScrollAnchor {
        key,
        offset_in_viewport,
    })
}

/// Applies a previously captured anchor by adjusting the scroll offset.
///
/// The adapter must provide a `key_to_index` mapping for the *current* dataset.
///
/// Returns `true` when the anchor was successfully applied.
pub fn apply_anchor<K: VirtualizerKey>(
    v: &mut virtualizer::Virtualizer<K>,
    anchor: &ScrollAnchor<K>,
    mut key_to_index: impl FnMut(&K) -> Option<usize>,
) -> bool {
    let Some(index) = key_to_index(&anchor.key) else {
        return false;
    };
    let Some(start) = v.item_start(index) else {
        return false;
    };
    let target = start.saturating_add(anchor.offset_in_viewport);
    v.set_scroll_offset_clamped(target);
    true
}
