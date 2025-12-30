use crate::{
    Easing, ScrollAnchor, Tween, VirtualizerKey, apply_anchor, capture_first_visible_anchor,
};

/// A framework-neutral controller that wraps a `virtualizer::Virtualizer` and provides common
/// adapter workflows (anchoring, tween-driven scrolling).
///
/// This type does not hold any UI objects. Adapters drive it by calling:
/// - `on_viewport_size` / `on_scroll` when UI events occur
/// - `tick(now_ms)` each frame/timer tick (for tween scrolling and `is_scrolling` debouncing)
///
/// For UI scroll containers (e.g. DOM), you can use the returned offset from `tick()` to set the
/// real scroll position, while keeping the virtualizer state in sync.
#[derive(Clone, Debug)]
pub struct Controller<K> {
    v: virtualizer::Virtualizer<K>,
    tween: Option<Tween>,
}

impl<K: VirtualizerKey> Controller<K> {
    pub fn new(options: virtualizer::VirtualizerOptions<K>) -> Self {
        Self {
            v: virtualizer::Virtualizer::new(options),
            tween: None,
        }
    }

    pub fn from_virtualizer(v: virtualizer::Virtualizer<K>) -> Self {
        Self { v, tween: None }
    }

    pub fn virtualizer(&self) -> &virtualizer::Virtualizer<K> {
        &self.v
    }

    pub fn virtualizer_mut(&mut self) -> &mut virtualizer::Virtualizer<K> {
        &mut self.v
    }

    pub fn into_virtualizer(self) -> virtualizer::Virtualizer<K> {
        self.v
    }

    pub fn is_animating(&self) -> bool {
        self.tween.is_some()
    }

    pub fn cancel_animation(&mut self) {
        self.tween = None;
    }

    pub fn on_viewport_size(&mut self, viewport_main: u32) {
        self.v.set_viewport_size(viewport_main);
    }

    /// Call this when the UI reports a scroll offset change (e.g. user wheel/drag).
    ///
    /// This cancels any active tween.
    pub fn on_scroll(&mut self, scroll_offset: u64, now_ms: u64) {
        self.cancel_animation();
        self.v.apply_scroll_offset_event(scroll_offset, now_ms);
    }

    /// Advances the controller.
    ///
    /// - If a tween is active, updates `scroll_offset` and returns the new offset.
    /// - Otherwise, runs `is_scrolling` debouncing and returns `None`.
    pub fn tick(&mut self, now_ms: u64) -> Option<u64> {
        let Some(tween) = self.tween else {
            self.v.update_scrolling(now_ms);
            return None;
        };

        let off = tween.sample(now_ms);
        self.v.apply_scroll_offset_event_clamped(off, now_ms);

        if tween.is_done(now_ms) {
            self.tween = None;
            self.v.set_is_scrolling(false);
        }

        Some(self.v.scroll_offset())
    }

    /// Computes and applies a scroll-to-index immediately (no animation).
    ///
    /// Returns the applied (clamped) offset.
    pub fn scroll_to_index(&mut self, index: usize, align: virtualizer::Align, now_ms: u64) -> u64 {
        let off = self.v.scroll_to_index_offset(index, align);
        self.v.apply_scroll_offset_event_clamped(off, now_ms);
        self.v.scroll_offset()
    }

    /// Applies a scroll-to-offset immediately (no animation).
    ///
    /// Returns the applied (clamped) offset.
    pub fn scroll_to_offset(&mut self, offset: u64, now_ms: u64) -> u64 {
        self.v.apply_scroll_offset_event_clamped(offset, now_ms);
        self.v.scroll_offset()
    }

    /// Starts a tween to an index (adapter-driven).
    ///
    /// Returns the clamped target offset.
    pub fn start_tween_to_index(
        &mut self,
        index: usize,
        align: virtualizer::Align,
        now_ms: u64,
        duration_ms: u64,
        easing: Easing,
    ) -> u64 {
        let to = self.v.scroll_to_index_offset(index, align);
        self.start_tween_to_offset(to, now_ms, duration_ms, easing)
    }

    /// Starts a tween to an offset (adapter-driven).
    ///
    /// Returns the clamped target offset.
    pub fn start_tween_to_offset(
        &mut self,
        offset: u64,
        now_ms: u64,
        duration_ms: u64,
        easing: Easing,
    ) -> u64 {
        let to = self.v.clamp_scroll_offset(offset);
        let from = self.v.scroll_offset();
        self.tween = Some(Tween::new(from, to, now_ms, duration_ms, easing));
        to
    }

    pub fn capture_first_visible_anchor(&self) -> Option<ScrollAnchor<K>> {
        capture_first_visible_anchor(&self.v)
    }

    /// Captures an anchor for the item at a given offset in the viewport.
    ///
    /// For example, `offset_in_viewport = 0` anchors the item at the top of the viewport.
    pub fn capture_anchor_at_offset_in_viewport(
        &self,
        offset_in_viewport: u64,
    ) -> Option<ScrollAnchor<K>> {
        let abs = self.v.scroll_offset().saturating_add(offset_in_viewport);
        let item = self.v.virtual_item_keyed_for_offset(abs)?;
        let offset_in_viewport = self.v.scroll_offset().saturating_sub(item.start);
        Some(ScrollAnchor {
            key: item.key,
            offset_in_viewport,
        })
    }

    /// Applies a previously captured anchor by adjusting the scroll offset.
    ///
    /// This cancels any active tween.
    pub fn apply_anchor(
        &mut self,
        anchor: &ScrollAnchor<K>,
        key_to_index: impl FnMut(&K) -> Option<usize>,
    ) -> bool {
        self.cancel_animation();
        apply_anchor(&mut self.v, anchor, key_to_index)
    }
}
