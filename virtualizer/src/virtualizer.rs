use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::Cell;
use core::cmp;

use crate::fenwick::Fenwick;
use crate::key::{KeyCacheKey, KeySizeMap};
use crate::{
    Align, InitialOffset, ItemKey, Range, Rect, ScrollDirection, VirtualItem, VirtualItemKeyed,
    VirtualRange, VirtualizerOptions,
};
use crate::{FrameState, ScrollState, ViewportState};

/// A headless virtualization engine.
///
/// This type is intentionally UI-agnostic:
/// - It does not hold any UI objects.
/// - Your adapter drives it by providing viewport geometry and scroll offsets.
/// - Rendering is exposed via zero-allocation iteration APIs (`for_each_virtual_*`).
///
/// For smooth scrolling / tweens / anchoring patterns, see the `virtualizer-adapter` crate.
#[derive(Clone, Debug)]
pub struct Virtualizer<K = ItemKey> {
    options: VirtualizerOptions<K>,
    viewport_size: u32,
    scroll_offset: u64,
    scroll_rect: Rect,
    is_scrolling: bool,
    scroll_direction: Option<ScrollDirection>,
    last_scroll_event_ms: Option<u64>,

    sizes: Vec<u32>, // base sizes (no gap)
    measured: Vec<bool>,
    sums: Fenwick,
    key_sizes: KeySizeMap<K>,

    notify_depth: Cell<usize>,
    notify_pending: Cell<bool>,
}

impl<K: KeyCacheKey> Virtualizer<K> {
    /// Creates a new virtualizer from options.
    ///
    /// If `options.initial_rect` and/or `options.initial_offset` are set, those values are applied
    /// immediately.
    pub fn new(options: VirtualizerOptions<K>) -> Self {
        let scroll_rect = options.initial_rect.unwrap_or_default();
        let scroll_offset = options.initial_offset.resolve();
        vdebug!(
            count = options.count,
            enabled = options.enabled,
            overscan = options.overscan,
            "Virtualizer::new"
        );
        let mut v = Self {
            viewport_size: scroll_rect.main,
            scroll_offset,
            scroll_rect,
            is_scrolling: false,
            scroll_direction: None,
            last_scroll_event_ms: None,
            sizes: Vec::new(),
            measured: Vec::new(),
            sums: Fenwick::new(0),
            key_sizes: KeySizeMap::<K>::new(),
            options,
            notify_depth: Cell::new(0),
            notify_pending: Cell::new(false),
        };
        v.rebuild_estimates();
        v
    }

    pub fn options(&self) -> &VirtualizerOptions<K> {
        &self.options
    }

    fn reset_to_initial(&mut self) {
        self.scroll_offset = self.options.initial_offset.resolve();
        self.scroll_rect = self.options.initial_rect.unwrap_or_default();
        self.viewport_size = self.scroll_rect.main;
        self.is_scrolling = false;
        self.scroll_direction = None;
        self.last_scroll_event_ms = None;
    }

    pub fn set_options(&mut self, options: VirtualizerOptions<K>) {
        let prev_count = self.options.count;
        let prev_gap = self.options.gap;
        let was_enabled = self.options.enabled;
        let estimate_size_unchanged =
            Arc::ptr_eq(&self.options.estimate_size, &options.estimate_size);
        let get_item_key_unchanged = Arc::ptr_eq(&self.options.get_item_key, &options.get_item_key);
        self.options = options;
        vtrace!(
            count = self.options.count,
            enabled = self.options.enabled,
            overscan = self.options.overscan,
            "Virtualizer::set_options"
        );

        if !self.options.enabled {
            self.viewport_size = 0;
            self.scroll_offset = self.options.initial_offset.resolve();
            self.scroll_rect = Rect::default();
            self.is_scrolling = false;
            self.scroll_direction = None;
            self.last_scroll_event_ms = None;
        } else if !was_enabled {
            self.reset_to_initial();
        } else if self.options.count != prev_count
            || !estimate_size_unchanged
            || !get_item_key_unchanged
        {
            self.rebuild_estimates();
        } else if self.options.gap != prev_gap {
            self.rebuild_fenwick();
        }

        self.notify();
    }

    /// Clones the current options, applies `f`, then delegates to `set_options`.
    ///
    /// This is useful when you want to update multiple options at once while letting the
    /// virtualizer decide what needs to be rebuilt (estimates/fenwick/reset).
    pub fn update_options(&mut self, f: impl FnOnce(&mut VirtualizerOptions<K>)) {
        let mut next = self.options.clone();
        f(&mut next);
        self.set_options(next);
    }

    pub fn set_on_change(
        &mut self,
        on_change: Option<impl Fn(&Virtualizer<K>, bool) + Send + Sync + 'static>,
    ) {
        self.options.on_change = on_change.map(|f| Arc::new(f) as _);
        self.notify();
    }

    pub fn set_initial_offset(&mut self, initial_offset: u64) {
        self.options.initial_offset = InitialOffset::Value(initial_offset);
        self.notify();
    }

    pub fn set_initial_offset_provider(
        &mut self,
        initial_offset: impl Fn() -> u64 + Send + Sync + 'static,
    ) {
        self.options.initial_offset = InitialOffset::Provider(Arc::new(initial_offset));
        self.notify();
    }

    pub fn set_use_scrollend_event(&mut self, use_scrollend_event: bool) {
        self.options.use_scrollend_event = use_scrollend_event;
        self.notify();
    }

    pub fn set_is_scrolling_reset_delay_ms(&mut self, delay_ms: u64) {
        self.options.is_scrolling_reset_delay_ms = delay_ms;
        self.notify();
    }

    fn notify_now(&self) {
        if let Some(cb) = &self.options.on_change {
            cb(self, self.is_scrolling);
        }
    }

    fn notify(&self) {
        if self.notify_depth.get() > 0 {
            self.notify_pending.set(true);
            return;
        }
        self.notify_now();
    }

    /// Batches multiple updates into a single `on_change` notification.
    ///
    /// This is recommended for UI adapters: on a typical frame, you might update the scroll
    /// rect, scroll offset, and `is_scrolling` state together. Without batching, each setter may
    /// trigger `on_change`, which can be expensive if the callback drives rendering.
    pub fn batch_update(&mut self, f: impl FnOnce(&mut Self)) {
        let depth = self.notify_depth.get();
        self.notify_depth.set(depth.saturating_add(1));

        f(self);

        let depth = self.notify_depth.get();
        debug_assert!(depth > 0, "notify_depth underflow");
        let next = depth.saturating_sub(1);
        self.notify_depth.set(next);

        if next == 0 && self.notify_pending.replace(false) {
            self.notify_now();
        }
    }

    pub fn count(&self) -> usize {
        self.options.count
    }

    pub fn enabled(&self) -> bool {
        self.options.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.options.enabled == enabled {
            return;
        }
        self.options.enabled = enabled;
        if !enabled {
            self.viewport_size = 0;
            self.scroll_offset = self.options.initial_offset.resolve();
            self.scroll_rect = Rect::default();
            self.is_scrolling = false;
            self.scroll_direction = None;
            self.last_scroll_event_ms = None;
        } else {
            self.reset_to_initial();
        }
        self.notify();
    }

    pub fn is_scrolling(&self) -> bool {
        self.is_scrolling
    }

    pub fn scroll_direction(&self) -> Option<ScrollDirection> {
        self.scroll_direction
    }

    pub fn set_is_scrolling(&mut self, is_scrolling: bool) {
        if self.is_scrolling == is_scrolling {
            return;
        }
        self.is_scrolling = is_scrolling;
        if !is_scrolling {
            self.scroll_direction = None;
            self.last_scroll_event_ms = None;
        }
        self.notify();
    }

    pub fn notify_scroll_event(&mut self, now_ms: u64) {
        if !self.options.enabled {
            return;
        }
        self.last_scroll_event_ms = Some(now_ms);
        self.set_is_scrolling(true);
    }

    pub fn update_scrolling(&mut self, now_ms: u64) {
        if !self.options.enabled {
            return;
        }
        if self.options.use_scrollend_event {
            return;
        }
        if !self.is_scrolling {
            return;
        }
        let Some(last) = self.last_scroll_event_ms else {
            return;
        };
        if now_ms.saturating_sub(last) >= self.options.is_scrolling_reset_delay_ms {
            self.set_is_scrolling(false);
        }
    }

    pub fn viewport_size(&self) -> u32 {
        self.viewport_size
    }

    pub fn scroll_rect(&self) -> Rect {
        self.scroll_rect
    }

    /// Returns a lightweight snapshot of the current viewport state.
    pub fn viewport_state(&self) -> ViewportState {
        ViewportState {
            rect: self.scroll_rect,
        }
    }

    /// Returns a lightweight snapshot of the current scroll state.
    pub fn scroll_state(&self) -> ScrollState {
        ScrollState {
            offset: self.scroll_offset,
            is_scrolling: self.is_scrolling,
        }
    }

    /// Returns a combined snapshot of viewport + scroll state.
    pub fn frame_state(&self) -> FrameState {
        FrameState {
            viewport: self.viewport_state(),
            scroll: self.scroll_state(),
        }
    }

    /// Restores viewport geometry from a previously captured snapshot.
    pub fn restore_viewport_state(&mut self, viewport: ViewportState) {
        self.set_scroll_rect(viewport.rect);
    }

    /// Restores scroll state from a previously captured snapshot.
    ///
    /// When `scroll.is_scrolling` is `true`, this will update the internal scrolling timers as if
    /// a scroll event happened at `now_ms`.
    pub fn restore_scroll_state(&mut self, scroll: ScrollState, now_ms: u64) {
        if scroll.is_scrolling {
            self.apply_scroll_offset_event_clamped(scroll.offset, now_ms);
            return;
        }
        self.batch_update(|v| {
            v.set_scroll_offset_clamped(scroll.offset);
            v.set_is_scrolling(false);
        });
    }

    /// Restores both viewport + scroll state from a previously captured snapshot.
    ///
    /// When `frame.scroll.is_scrolling` is `true`, this will update the internal scrolling timers
    /// as if a scroll event happened at `now_ms`.
    pub fn restore_frame_state(&mut self, frame: FrameState, now_ms: u64) {
        if frame.scroll.is_scrolling {
            self.apply_scroll_frame_clamped(frame.viewport.rect, frame.scroll.offset, now_ms);
            return;
        }
        self.batch_update(|v| {
            v.set_scroll_rect(frame.viewport.rect);
            v.set_scroll_offset_clamped(frame.scroll.offset);
            v.set_is_scrolling(false);
        });
    }

    pub fn set_scroll_rect(&mut self, rect: Rect) {
        if self.scroll_rect == rect {
            return;
        }
        self.scroll_rect = rect;
        self.viewport_size = rect.main;
        self.notify();
    }

    /// Applies a scroll rect update from your UI layer.
    ///
    /// Prefer this (or `apply_scroll_frame*`) over calling multiple setters when you have an
    /// `on_change` callback that may trigger expensive work (like rendering/layout).
    pub fn apply_scroll_rect_event(&mut self, rect: Rect) {
        self.batch_update(|v| {
            v.set_scroll_rect(rect);
        });
    }

    pub fn scroll_offset(&self) -> u64 {
        self.scroll_offset
    }

    pub fn scroll_offset_in_list(&self) -> u64 {
        let margin = self.options.scroll_margin as u64;
        self.scroll_offset.saturating_sub(margin)
    }

    pub fn set_viewport_size(&mut self, size: u32) {
        if self.viewport_size == size && self.scroll_rect.main == size {
            return;
        }
        self.viewport_size = size;
        self.scroll_rect.main = size;
        self.notify();
    }

    pub fn set_scroll_offset(&mut self, offset: u64) {
        if self.scroll_offset == offset {
            return;
        }
        let prev = self.scroll_offset;
        self.scroll_offset = offset;
        self.scroll_direction = match offset.cmp(&prev) {
            cmp::Ordering::Greater => Some(ScrollDirection::Forward),
            cmp::Ordering::Less => Some(ScrollDirection::Backward),
            cmp::Ordering::Equal => self.scroll_direction,
        };
        self.notify();
    }

    /// Applies a scroll offset update from your UI layer (e.g. wheel/drag), and marks the
    /// virtualizer as scrolling.
    pub fn apply_scroll_offset_event(&mut self, offset: u64, now_ms: u64) {
        vtrace!(offset, now_ms, "apply_scroll_offset_event");
        self.batch_update(|v| {
            v.set_scroll_offset(offset);
            v.notify_scroll_event(now_ms);
        });
    }

    pub fn set_scroll_offset_clamped(&mut self, offset: u64) {
        let clamped = self.clamp_scroll_offset(offset);
        self.set_scroll_offset(clamped);
    }

    /// Same as `apply_scroll_offset_event`, but clamps the offset.
    pub fn apply_scroll_offset_event_clamped(&mut self, offset: u64, now_ms: u64) {
        vtrace!(offset, now_ms, "apply_scroll_offset_event_clamped");
        self.batch_update(|v| {
            v.set_scroll_offset_clamped(offset);
            v.notify_scroll_event(now_ms);
        });
    }

    pub fn set_viewport_and_scroll(&mut self, viewport_size: u32, scroll_offset: u64) {
        self.batch_update(|v| {
            v.set_viewport_size(viewport_size);
            v.set_scroll_offset(scroll_offset);
        });
    }

    pub fn set_viewport_and_scroll_clamped(&mut self, viewport_size: u32, scroll_offset: u64) {
        self.batch_update(|v| {
            v.set_viewport_size(viewport_size);
            v.set_scroll_offset_clamped(scroll_offset);
        });
    }

    /// Applies both scroll rect and scroll offset in a single coalesced update.
    ///
    /// This is the recommended entry point for UI adapters that receive scroll events along with
    /// updated viewport/rect information.
    pub fn apply_scroll_frame(&mut self, rect: Rect, scroll_offset: u64, now_ms: u64) {
        vtrace!(
            rect_main = rect.main,
            rect_cross = rect.cross,
            scroll_offset,
            now_ms,
            "apply_scroll_frame"
        );
        self.batch_update(|v| {
            v.set_scroll_rect(rect);
            v.set_scroll_offset(scroll_offset);
            v.notify_scroll_event(now_ms);
        });
    }

    /// Same as `apply_scroll_frame`, but clamps the offset.
    pub fn apply_scroll_frame_clamped(&mut self, rect: Rect, scroll_offset: u64, now_ms: u64) {
        vtrace!(
            rect_main = rect.main,
            rect_cross = rect.cross,
            scroll_offset,
            now_ms,
            "apply_scroll_frame_clamped"
        );
        self.batch_update(|v| {
            v.set_scroll_rect(rect);
            v.set_scroll_offset_clamped(scroll_offset);
            v.notify_scroll_event(now_ms);
        });
    }

    pub fn set_count(&mut self, count: usize) {
        if self.options.count == count {
            return;
        }
        self.options.count = count;
        self.rebuild_estimates();
        self.notify();
    }

    pub fn set_overscan(&mut self, overscan: usize) {
        self.options.overscan = overscan;
        self.notify();
    }

    pub fn set_padding(&mut self, padding_start: u32, padding_end: u32) {
        self.options.padding_start = padding_start;
        self.options.padding_end = padding_end;
        self.notify();
    }

    pub fn set_scroll_padding(&mut self, scroll_padding_start: u32, scroll_padding_end: u32) {
        self.options.scroll_padding_start = scroll_padding_start;
        self.options.scroll_padding_end = scroll_padding_end;
        self.notify();
    }

    pub fn set_scroll_margin(&mut self, scroll_margin: u32) {
        self.options.scroll_margin = scroll_margin;
        self.notify();
    }

    pub fn set_gap(&mut self, gap: u32) {
        if self.options.gap == gap {
            return;
        }
        self.options.gap = gap;
        self.rebuild_fenwick();
        self.notify();
    }

    pub fn set_get_item_key(&mut self, f: impl Fn(usize) -> K + Send + Sync + 'static) {
        self.options.get_item_key = Arc::new(f);
        self.rebuild_estimates();
        self.notify();
    }

    pub fn set_should_adjust_scroll_position_on_item_size_change(
        &mut self,
        f: Option<impl Fn(&Virtualizer<K>, VirtualItem, i64) -> bool + Send + Sync + 'static>,
    ) {
        self.options
            .should_adjust_scroll_position_on_item_size_change = f.map(|f| Arc::new(f) as _);
        self.notify();
    }

    pub fn sync_item_keys(&mut self) {
        // Rebuild per-index sizes from the key-based cache and current estimates.
        // Call this after your data set is reordered/changed while `count` stays the same.
        let count = self.options.count;
        self.sizes.clear();
        self.measured.clear();
        self.sizes.reserve_exact(count);
        self.measured.reserve_exact(count);

        for i in 0..count {
            let key = self.key_for(i);
            if let Some(&measured_size) = self.key_sizes.get(&key) {
                self.sizes.push(measured_size);
                self.measured.push(true);
            } else {
                self.sizes.push((self.options.estimate_size)(i));
                self.measured.push(false);
            }
        }

        self.rebuild_fenwick();
        self.notify();
    }

    pub fn set_range_extractor(
        &mut self,
        f: Option<impl Fn(Range, &mut dyn FnMut(usize)) + Send + Sync + 'static>,
    ) {
        self.options.range_extractor = f.map(|f| Arc::new(f) as _);
        self.notify();
    }

    pub fn set_estimate_size(&mut self, f: impl Fn(usize) -> u32 + Send + Sync + 'static) {
        self.options.estimate_size = Arc::new(f);
        self.rebuild_estimates();
        self.notify();
    }

    pub fn reset_measurements(&mut self) {
        self.key_sizes.clear();
        self.rebuild_estimates();
        self.notify();
    }

    /// Returns the number of cached measured sizes (key → size).
    pub fn measurement_cache_len(&self) -> usize {
        self.key_sizes.len()
    }

    /// Iterates over the cached measured sizes (key → size) without allocations.
    pub fn for_each_cached_size(&self, mut f: impl FnMut(&K, u32)) {
        for (k, v) in self.key_sizes.iter() {
            f(k, *v);
        }
    }

    /// Exports the cached measured sizes as a `Vec` (useful for persistence).
    pub fn export_measurement_cache(&self) -> Vec<(K, u32)>
    where
        K: Clone,
    {
        let mut out = Vec::with_capacity(self.key_sizes.len());
        self.for_each_cached_size(|k, v| out.push((k.clone(), v)));
        out
    }

    /// Replaces the cached measured sizes from an iterator (useful when restoring state).
    ///
    /// Note: this rebuilds internal per-index sizes using the current key mapping.
    pub fn import_measurement_cache(&mut self, entries: impl IntoIterator<Item = (K, u32)>) {
        self.key_sizes.clear();
        let mut n = 0usize;
        for (k, v) in entries {
            self.key_sizes.insert(k, v);
            n = n.saturating_add(1);
        }
        vdebug!(entries = n, "import_measurement_cache");
        self.rebuild_estimates();
        self.notify();
    }

    pub fn measure(&mut self, index: usize, size: u32) {
        if index >= self.options.count {
            return;
        }
        let key = self.key_for(index);
        self.measure_keyed(index, key, size);
    }

    pub fn measure_keyed(&mut self, index: usize, key: K, size: u32) {
        if index >= self.options.count {
            return;
        }
        vtrace!(index, size, "measure_keyed");
        self.set_item_size_keyed(index, key, size);
        self.notify();
    }

    pub fn resize_item(&mut self, index: usize, size: u32) -> i64 {
        if index >= self.options.count {
            return 0;
        }
        let key = self.key_for(index);
        self.resize_item_keyed(index, key, size)
    }

    pub fn resize_item_keyed(&mut self, index: usize, key: K, size: u32) -> i64 {
        if index >= self.options.count {
            return 0;
        }
        let item = self.item(index);
        let delta = self.set_item_size_keyed(index, key, size);
        if delta == 0 {
            self.notify();
            return 0;
        }

        let should_adjust = if let Some(f) = &self
            .options
            .should_adjust_scroll_position_on_item_size_change
        {
            f(self, item, delta)
        } else {
            item.start < self.scroll_offset
        };

        if should_adjust {
            if delta > 0 {
                self.scroll_offset = self.scroll_offset.saturating_add(delta as u64);
            } else {
                self.scroll_offset = self.scroll_offset.saturating_sub((-delta) as u64);
            }
            self.notify();
            delta
        } else {
            self.notify();
            0
        }
    }

    fn set_item_size_keyed(&mut self, index: usize, key: K, size: u32) -> i64 {
        let cur = self.sizes[index];
        if cur == size {
            self.measured[index] = true;
            self.key_sizes.insert(key, size);
            return 0;
        }
        self.sizes[index] = size;
        self.measured[index] = true;
        self.key_sizes.insert(key, size);
        let delta = size as i64 - cur as i64;
        self.sums.add(index, delta);
        delta
    }

    pub fn measure_many(&mut self, measurements: impl IntoIterator<Item = (usize, u32)>) {
        for (index, size) in measurements {
            if index >= self.options.count {
                continue;
            }
            let key = self.key_for(index);
            let cur = self.sizes[index];
            if cur == size {
                self.measured[index] = true;
                self.key_sizes.insert(key, size);
                continue;
            }
            self.sizes[index] = size;
            self.measured[index] = true;
            self.key_sizes.insert(key, size);
            self.sums.add(index, size as i64 - cur as i64);
        }
        self.notify();
    }

    pub fn resize_item_many(
        &mut self,
        measurements: impl IntoIterator<Item = (usize, u32)>,
    ) -> i64 {
        let mut applied = 0i64;
        for (index, size) in measurements {
            if index >= self.options.count {
                continue;
            }
            applied += self.resize_item(index, size);
        }
        applied
    }

    pub fn is_measured(&self, index: usize) -> bool {
        self.measured.get(index).copied().unwrap_or(false)
    }

    pub fn total_size(&self) -> u64 {
        if !self.options.enabled {
            return 0;
        }
        self.options.padding_start as u64 + self.sums.total() + self.options.padding_end as u64
    }

    pub fn key_for(&self, index: usize) -> K {
        (self.options.get_item_key)(index)
    }

    pub fn virtual_range(&self) -> VirtualRange {
        if !self.options.enabled {
            return VirtualRange {
                start_index: 0,
                end_index: 0,
            };
        }
        self.compute_range(self.scroll_offset, self.viewport_size)
    }

    pub fn virtual_range_for(&self, scroll_offset: u64, viewport_size: u32) -> VirtualRange {
        if !self.options.enabled {
            return VirtualRange {
                start_index: 0,
                end_index: 0,
            };
        }
        self.compute_range(scroll_offset, viewport_size)
    }

    pub fn visible_range(&self) -> VirtualRange {
        if !self.options.enabled {
            return VirtualRange {
                start_index: 0,
                end_index: 0,
            };
        }
        self.compute_visible_range(self.scroll_offset, self.viewport_size)
    }

    pub fn visible_range_for(&self, scroll_offset: u64, viewport_size: u32) -> VirtualRange {
        if !self.options.enabled {
            return VirtualRange {
                start_index: 0,
                end_index: 0,
            };
        }
        self.compute_visible_range(scroll_offset, viewport_size)
    }

    pub fn for_each_virtual_index(&self, f: impl FnMut(usize)) {
        self.for_each_virtual_index_for(self.scroll_offset, self.viewport_size, f);
    }

    pub fn for_each_virtual_index_for(
        &self,
        scroll_offset: u64,
        viewport_size: u32,
        mut f: impl FnMut(usize),
    ) {
        if !self.options.enabled {
            return;
        }

        let visible = self.visible_range_for(scroll_offset, viewport_size);
        if visible.is_empty() {
            return;
        }

        let count = self.options.count;
        let range = Range {
            start_index: visible.start_index,
            end_index: visible.end_index,
            overscan: self.options.overscan,
            count,
        };

        if let Some(extract) = &self.options.range_extractor {
            let mut prev: Option<usize> = None;
            extract(range, &mut |i| {
                if i >= count {
                    debug_assert!(
                        i < count,
                        "range_extractor emitted out-of-bounds index (i={i}, count={count})"
                    );
                    return;
                }
                if let Some(p) = prev {
                    if i == p {
                        return;
                    }
                    if i < p {
                        debug_assert!(
                            i > p,
                            "range_extractor must emit sorted indexes (prev={p}, next={i})"
                        );
                        return;
                    }
                    debug_assert!(
                        i > p,
                        "range_extractor must emit sorted indexes (prev={p}, next={i})"
                    );
                }
                prev = Some(i);
                f(i);
            });
            return;
        }

        let overscan = self.options.overscan;
        let start = visible.start_index.saturating_sub(overscan);
        let end = cmp::min(count, visible.end_index.saturating_add(overscan));
        for i in start..end {
            f(i);
        }
    }

    pub fn for_each_virtual_item(&self, f: impl FnMut(VirtualItem)) {
        self.for_each_virtual_item_for(self.scroll_offset, self.viewport_size, f);
    }

    pub fn for_each_virtual_item_for(
        &self,
        scroll_offset: u64,
        viewport_size: u32,
        mut f: impl FnMut(VirtualItem),
    ) {
        if !self.options.enabled {
            return;
        }

        let visible = self.visible_range_for(scroll_offset, viewport_size);
        if visible.is_empty() {
            return;
        }

        if self.options.range_extractor.is_some() {
            self.for_each_virtual_index_for(scroll_offset, viewport_size, |i| {
                f(self.item(i));
            });
            return;
        }

        let count = self.options.count;
        let overscan = self.options.overscan;
        let start_index = visible.start_index.saturating_sub(overscan);
        let end_index = cmp::min(count, visible.end_index.saturating_add(overscan));
        if start_index >= end_index {
            return;
        }

        let margin = self.options.scroll_margin as u64;
        let mut start = margin.saturating_add(self.start_of(start_index));
        let gap = self.options.gap as u64;

        for i in start_index..end_index {
            let size = self.sizes[i];
            f(VirtualItem {
                index: i,
                start,
                size,
            });

            start = start.saturating_add(size as u64);
            if gap > 0 && i + 1 < count {
                start = start.saturating_add(gap);
            }
        }
    }

    pub fn for_each_virtual_item_keyed(&self, f: impl FnMut(VirtualItemKeyed<K>)) {
        self.for_each_virtual_item_keyed_for(self.scroll_offset, self.viewport_size, f);
    }

    pub fn for_each_virtual_item_keyed_for(
        &self,
        scroll_offset: u64,
        viewport_size: u32,
        mut f: impl FnMut(VirtualItemKeyed<K>),
    ) {
        if !self.options.enabled {
            return;
        }

        let visible = self.visible_range_for(scroll_offset, viewport_size);
        if visible.is_empty() {
            return;
        }

        if self.options.range_extractor.is_some() {
            self.for_each_virtual_index_for(scroll_offset, viewport_size, |i| {
                let item = self.item(i);
                f(VirtualItemKeyed {
                    key: self.key_for(i),
                    index: item.index,
                    start: item.start,
                    size: item.size,
                });
            });
            return;
        }

        let count = self.options.count;
        let overscan = self.options.overscan;
        let start_index = visible.start_index.saturating_sub(overscan);
        let end_index = cmp::min(count, visible.end_index.saturating_add(overscan));
        if start_index >= end_index {
            return;
        }

        let margin = self.options.scroll_margin as u64;
        let mut start = margin.saturating_add(self.start_of(start_index));
        let gap = self.options.gap as u64;

        for i in start_index..end_index {
            let size = self.sizes[i];
            f(VirtualItemKeyed {
                key: self.key_for(i),
                index: i,
                start,
                size,
            });

            start = start.saturating_add(size as u64);
            if gap > 0 && i + 1 < count {
                start = start.saturating_add(gap);
            }
        }
    }

    /// Programmatically scrolls to an index (no animation).
    ///
    /// This sets the internal `scroll_offset` to the computed (clamped) target and triggers
    /// `on_change`. It does **not** mark the virtualizer as "scrolling".
    ///
    /// If you want "user scrolling" semantics (e.g. to drive `is_scrolling` debouncing), call
    /// `apply_scroll_offset_event_clamped(scroll_to_index_offset(...), now_ms)` instead.
    ///
    /// Returns the applied (clamped) offset.
    pub fn scroll_to_index(&mut self, index: usize, align: Align) -> u64 {
        let offset = self.scroll_to_index_offset(index, align);
        self.set_scroll_offset(offset);
        offset
    }

    pub fn scroll_to_index_offset(&self, index: usize, align: Align) -> u64 {
        if !self.options.enabled {
            return self.options.initial_offset.resolve();
        }
        if self.options.count == 0 {
            return 0;
        }
        let index = index.min(self.options.count - 1);
        let item = self.item(index);

        let sp_start = self.options.scroll_padding_start as u64;
        let sp_end = self.options.scroll_padding_end as u64;
        let view = self.viewport_size as u64;

        let target = match align {
            Align::Start => item.start.saturating_sub(sp_start),
            Align::End => item.end().saturating_add(sp_end).saturating_sub(view),
            Align::Center => {
                let center = item.start.saturating_add(item.size as u64 / 2);
                center.saturating_sub(view / 2)
            }
            Align::Auto => {
                let cur = self.scroll_offset;
                let cur_end = cur.saturating_add(view);
                if item.start >= cur && item.end() <= cur_end {
                    cur
                } else if item.start < cur {
                    item.start.saturating_sub(sp_start)
                } else {
                    item.end().saturating_add(sp_end).saturating_sub(view)
                }
            }
        };

        self.clamp_scroll_offset(target)
    }

    /// Collects virtual item indexes into `out` (clears `out` first).
    ///
    /// This is a convenience wrapper around [`Self::for_each_virtual_index`]. For maximum
    /// performance, prefer `for_each_virtual_index` and reuse a scratch buffer in your adapter.
    pub fn collect_virtual_indexes(&self, out: &mut Vec<usize>) {
        self.collect_virtual_indexes_for(self.scroll_offset, self.viewport_size, out);
    }

    /// Collects virtual item indexes into `out` for a given `scroll_offset`/`viewport_size`.
    ///
    /// This clears `out` first.
    pub fn collect_virtual_indexes_for(
        &self,
        scroll_offset: u64,
        viewport_size: u32,
        out: &mut Vec<usize>,
    ) {
        out.clear();
        self.for_each_virtual_index_for(scroll_offset, viewport_size, |i| out.push(i));
    }

    /// Collects virtual items into `out` (clears `out` first).
    ///
    /// This is a convenience wrapper around [`Self::for_each_virtual_item`]. For maximum
    /// performance, prefer `for_each_virtual_item` and reuse a scratch buffer in your adapter.
    pub fn collect_virtual_items(&self, out: &mut Vec<VirtualItem>) {
        self.collect_virtual_items_for(self.scroll_offset, self.viewport_size, out);
    }

    /// Collects virtual items into `out` for a given `scroll_offset`/`viewport_size`.
    ///
    /// This clears `out` first.
    pub fn collect_virtual_items_for(
        &self,
        scroll_offset: u64,
        viewport_size: u32,
        out: &mut Vec<VirtualItem>,
    ) {
        out.clear();
        self.for_each_virtual_item_for(scroll_offset, viewport_size, |it| out.push(it));
    }

    /// Collects keyed virtual items into `out` (clears `out` first).
    ///
    /// This is a convenience wrapper around [`Self::for_each_virtual_item_keyed`].
    pub fn collect_virtual_items_keyed(&self, out: &mut Vec<VirtualItemKeyed<K>>) {
        self.collect_virtual_items_keyed_for(self.scroll_offset, self.viewport_size, out);
    }

    /// Collects keyed virtual items into `out` for a given `scroll_offset`/`viewport_size`.
    ///
    /// This clears `out` first.
    pub fn collect_virtual_items_keyed_for(
        &self,
        scroll_offset: u64,
        viewport_size: u32,
        out: &mut Vec<VirtualItemKeyed<K>>,
    ) {
        out.clear();
        self.for_each_virtual_item_keyed_for(scroll_offset, viewport_size, |it| out.push(it));
    }

    pub fn index_at_offset(&self, offset: u64) -> Option<usize> {
        if !self.options.enabled {
            return None;
        }
        self.index_at_offset_inner(offset)
            .filter(|&i| i < self.options.count)
    }

    pub fn item_start(&self, index: usize) -> Option<u64> {
        if !self.options.enabled {
            return None;
        }
        (index < self.options.count).then(|| {
            let margin = self.options.scroll_margin as u64;
            margin.saturating_add(self.start_of(index))
        })
    }

    pub fn item_size(&self, index: usize) -> Option<u32> {
        if !self.options.enabled {
            return None;
        }
        self.sizes.get(index).copied()
    }

    pub fn item_end(&self, index: usize) -> Option<u64> {
        let start = self.item_start(index)?;
        let size = self.item_size(index)? as u64;
        Some(start.saturating_add(size))
    }

    pub fn virtual_item_for_offset(&self, offset: u64) -> Option<VirtualItem> {
        let index = self.index_at_offset(offset)?;
        Some(self.item(index))
    }

    pub fn virtual_item_keyed_for_offset(&self, offset: u64) -> Option<VirtualItemKeyed<K>> {
        let index = self.index_at_offset(offset)?;
        let item = self.item(index);
        Some(VirtualItemKeyed {
            key: self.key_for(index),
            index: item.index,
            start: item.start,
            size: item.size,
        })
    }

    fn rebuild_estimates(&mut self) {
        vdebug!(
            count = self.options.count,
            cached = self.key_sizes.len(),
            "rebuild_estimates"
        );
        self.sizes.clear();
        self.measured.clear();
        self.sizes
            .reserve_exact(self.options.count.saturating_sub(self.sizes.len()));
        self.measured
            .reserve_exact(self.options.count.saturating_sub(self.measured.len()));

        for i in 0..self.options.count {
            let key = self.key_for(i);
            if let Some(&measured_size) = self.key_sizes.get(&key) {
                self.sizes.push(measured_size);
                self.measured.push(true);
            } else {
                self.sizes.push((self.options.estimate_size)(i));
                self.measured.push(false);
            }
        }
        self.rebuild_fenwick();
    }

    fn rebuild_fenwick(&mut self) {
        self.sums = Fenwick::from_sizes(&self.sizes, self.options.gap);
    }

    fn item(&self, index: usize) -> VirtualItem {
        let margin = self.options.scroll_margin as u64;
        let start = margin.saturating_add(self.start_of(index));
        VirtualItem {
            index,
            start,
            size: self.sizes[index],
        }
    }

    fn start_of(&self, index: usize) -> u64 {
        self.options.padding_start as u64 + self.sums.prefix_sum(index)
    }

    pub fn max_scroll_offset(&self) -> u64 {
        if !self.options.enabled {
            return self.options.initial_offset.resolve();
        }
        let margin = self.options.scroll_margin as u64;
        let total = self.total_size();
        let view = self.viewport_size as u64;
        margin.saturating_add(total.saturating_sub(view))
    }

    pub fn clamp_scroll_offset(&self, offset: u64) -> u64 {
        offset.min(self.max_scroll_offset())
    }

    fn compute_range(&self, scroll_offset: u64, viewport_size: u32) -> VirtualRange {
        let mut range = self.compute_visible_range(scroll_offset, viewport_size);
        if range.is_empty() {
            return range;
        }

        let count = self.options.count;
        let overscan = self.options.overscan;
        range.start_index = range.start_index.saturating_sub(overscan);
        range.end_index = cmp::min(count, range.end_index.saturating_add(overscan));
        range
    }

    fn compute_visible_range(&self, scroll_offset: u64, viewport_size: u32) -> VirtualRange {
        let count = self.options.count;
        if count == 0 || viewport_size == 0 {
            return VirtualRange {
                start_index: 0,
                end_index: 0,
            };
        }

        let margin = self.options.scroll_margin as u64;
        let view = viewport_size as u64;

        let total = self.total_size();
        let max_scroll = margin.saturating_add(total.saturating_sub(view));
        let scroll_offset = scroll_offset.min(max_scroll);
        let scroll_end = scroll_offset.saturating_add(view);
        if scroll_end <= margin {
            return VirtualRange {
                start_index: 0,
                end_index: 0,
            };
        }

        let off = scroll_offset.saturating_sub(margin);
        let visible_end_exclusive = scroll_end.saturating_sub(margin);

        if off >= total {
            return VirtualRange {
                start_index: count,
                end_index: count,
            };
        }

        let visible_start = off;
        let visible_end_inclusive = visible_end_exclusive.saturating_sub(1);

        let mut start = self
            .index_at_offset_inner_list(visible_start)
            .unwrap_or(count);
        let mut end = self
            .index_at_offset_inner_list(cmp::max(visible_end_inclusive, visible_start))
            .map(|i| i + 1)
            .unwrap_or(count);

        start = start.min(count);
        end = end.min(count);

        VirtualRange {
            start_index: start,
            end_index: end,
        }
    }

    fn index_at_offset_inner(&self, offset: u64) -> Option<usize> {
        let margin = self.options.scroll_margin as u64;
        if offset < margin {
            return Some(0);
        }
        self.index_at_offset_inner_list(offset - margin)
    }

    fn index_at_offset_inner_list(&self, offset: u64) -> Option<usize> {
        let ps = self.options.padding_start as u64;
        if offset < ps {
            return Some(0);
        }

        let off_in_items = offset - ps;
        let count = self.options.count;
        if count == 0 {
            return None;
        }

        // Find the first item whose (effective) end is > off_in_items.
        // Fenwick lower_bound returns the number of items whose prefix sum is <= off_in_items.
        let consumed = self.sums.lower_bound(off_in_items);
        Some(consumed.min(count.saturating_sub(1)))
    }
}
