//! A headless virtualization engine inspired by TanStack Virtual.
//!
//! This crate focuses on the core algorithms needed to render massive lists at interactive
//! frame rates: prefix sums over item sizes, fast offset → index lookup, overscanned visible
//! ranges, and optional dynamic measurement.
//!
//! It is UI-agnostic. A TUI/GUI layer is expected to provide:
//! - viewport size (height/width)
//! - scroll offset
//! - item size estimates and (optionally) dynamic measurements
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cmp;

#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap;
#[cfg(feature = "std")]
use std::collections::HashMap;

#[cfg(feature = "std")]
type KeySizeMap<K> = HashMap<K, u32>;
#[cfg(not(feature = "std"))]
type KeySizeMap<K> = BTreeMap<K, u32>;

#[cfg(feature = "std")]
#[doc(hidden)]
pub trait KeyCacheKey: core::hash::Hash + Eq {}
#[cfg(feature = "std")]
impl<K: core::hash::Hash + Eq> KeyCacheKey for K {}

#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub trait KeyCacheKey: Ord {}
#[cfg(not(feature = "std"))]
impl<K: Ord> KeyCacheKey for K {}

#[cfg(test)]
extern crate std;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollDirection {
    Forward,
    Backward,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub main: u32,
    pub cross: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
pub struct Range {
    pub start_index: usize,
    pub end_index: usize, // exclusive, visible range (no overscan)
    pub overscan: usize,
    pub count: usize,
}

pub type OnChangeCallback<K> = Arc<dyn Fn(&Virtualizer<K>, bool) + Send + Sync>;

pub type ShouldAdjustScrollPositionOnItemSizeChangeCallback<K> =
    Arc<dyn Fn(&Virtualizer<K>, VirtualItem, i64) -> bool + Send + Sync>;

pub type RangeExtractorV2 = Arc<dyn Fn(Range) -> Vec<usize> + Send + Sync>;

#[derive(Clone)]
pub enum InitialOffset {
    Value(u64),
    Provider(Arc<dyn Fn() -> u64 + Send + Sync>),
}

impl InitialOffset {
    fn resolve(&self) -> u64 {
        match self {
            Self::Value(v) => *v,
            Self::Provider(f) => f(),
        }
    }
}

impl Default for InitialOffset {
    fn default() -> Self {
        Self::Value(0)
    }
}

impl core::fmt::Debug for InitialOffset {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Value(v) => f.debug_tuple("Value").field(v).finish(),
            Self::Provider(_) => f.write_str("Provider(..)"),
        }
    }
}

#[derive(Clone)]
pub struct VirtualizerOptions<K = ItemKey> {
    pub count: usize,
    pub estimate_size: Arc<dyn Fn(usize) -> u32 + Send + Sync>,
    pub get_item_key: Arc<dyn Fn(usize) -> K + Send + Sync>,
    pub range_extractor: Option<Arc<dyn Fn(VirtualRange) -> Vec<usize> + Send + Sync>>,
    pub range_extractor_v2: Option<RangeExtractorV2>,

    /// Enables/disables the virtualizer. When disabled, query methods return empty results.
    pub enabled: bool,
    /// Enables debug mode (intended for adapter-level logging).
    pub debug: bool,

    pub overscan: usize,

    /// The initial size of the scrollable area (aka TanStack Virtual `initialRect`).
    ///
    /// This is a platform-agnostic rect where:
    /// - `main` is the virtualized axis size (e.g. height for vertical lists)
    /// - `cross` is the cross axis size (e.g. width for vertical lists)
    pub initial_rect: Option<Rect>,

    /// Padding before the first item.
    pub padding_start: u32,
    /// Padding after the last item.
    pub padding_end: u32,

    /// Additional padding applied when computing scroll-to offsets.
    pub scroll_padding_start: u32,
    /// Additional padding applied when computing scroll-to offsets.
    pub scroll_padding_end: u32,

    /// Where the list starts inside the scroll element (aka TanStack Virtual `scrollMargin`).
    ///
    /// This is useful when the scroll offset is measured from a larger scroll container (e.g.
    /// window scrolling) while the list begins after some header/content.
    pub scroll_margin: u32,

    /// Initial scroll offset (aka TanStack Virtual `initialOffset`).
    pub initial_offset: InitialOffset,

    /// Optional callback fired when the virtualizer's internal state changes.
    ///
    /// The `sync` argument indicates whether a scroll is in progress.
    pub on_change: Option<OnChangeCallback<K>>,

    /// Determines whether to use a native scrollend event to detect when scrolling has stopped.
    ///
    /// This is included for TanStack Virtual parity. In this crate, scrolling state is driven
    /// by your adapter via `set_is_scrolling`/`notify_scroll_event`/`update_scrolling`.
    pub use_scrollend_event: bool,

    /// Debounced fallback duration for resetting `is_scrolling` when `use_scrollend_event` is false.
    pub is_scrolling_reset_delay_ms: u64,

    /// Whether to adjust the scroll position when an item's measured size differs from its
    /// estimate and the item is before the current scroll offset.
    pub should_adjust_scroll_position_on_item_size_change:
        Option<ShouldAdjustScrollPositionOnItemSizeChangeCallback<K>>,

    /// Space between items.
    pub gap: u32,

    /// Orientation hint (does not change math; kept for parity with TanStack Virtual).
    pub horizontal: bool,
}

impl VirtualizerOptions<ItemKey> {
    pub fn new(count: usize, estimate_size: impl Fn(usize) -> u32 + Send + Sync + 'static) -> Self {
        Self {
            count,
            estimate_size: Arc::new(estimate_size),
            get_item_key: Arc::new(|i| i as u64),
            range_extractor: None,
            range_extractor_v2: None,
            enabled: true,
            debug: false,
            overscan: 1,
            initial_rect: None,
            padding_start: 0,
            padding_end: 0,
            scroll_padding_start: 0,
            scroll_padding_end: 0,
            scroll_margin: 0,
            initial_offset: InitialOffset::default(),
            on_change: None,
            use_scrollend_event: false,
            is_scrolling_reset_delay_ms: 150,
            should_adjust_scroll_position_on_item_size_change: None,
            gap: 0,
            horizontal: false,
        }
    }
}

impl<K> VirtualizerOptions<K> {
    pub fn new_with_key(
        count: usize,
        estimate_size: impl Fn(usize) -> u32 + Send + Sync + 'static,
        get_item_key: impl Fn(usize) -> K + Send + Sync + 'static,
    ) -> Self {
        Self {
            count,
            estimate_size: Arc::new(estimate_size),
            get_item_key: Arc::new(get_item_key),
            range_extractor: None,
            range_extractor_v2: None,
            enabled: true,
            debug: false,
            overscan: 1,
            initial_rect: None,
            padding_start: 0,
            padding_end: 0,
            scroll_padding_start: 0,
            scroll_padding_end: 0,
            scroll_margin: 0,
            initial_offset: InitialOffset::default(),
            on_change: None,
            use_scrollend_event: false,
            is_scrolling_reset_delay_ms: 150,
            should_adjust_scroll_position_on_item_size_change: None,
            gap: 0,
            horizontal: false,
        }
    }

    pub fn with_get_item_key(
        mut self,
        get_item_key: impl Fn(usize) -> K + Send + Sync + 'static,
    ) -> Self {
        self.get_item_key = Arc::new(get_item_key);
        self
    }

    pub fn with_range_extractor(
        mut self,
        range_extractor: Option<impl Fn(VirtualRange) -> Vec<usize> + Send + Sync + 'static>,
    ) -> Self {
        self.range_extractor = range_extractor.map(|f| Arc::new(f) as _);
        self
    }

    pub fn with_range_extractor_v2(
        mut self,
        range_extractor: Option<impl Fn(Range) -> Vec<usize> + Send + Sync + 'static>,
    ) -> Self {
        self.range_extractor_v2 = range_extractor.map(|f| Arc::new(f) as _);
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    pub fn with_initial_rect(mut self, initial_rect: Option<Rect>) -> Self {
        self.initial_rect = initial_rect;
        self
    }

    pub fn with_overscan(mut self, overscan: usize) -> Self {
        self.overscan = overscan;
        self
    }

    pub fn with_padding(mut self, padding_start: u32, padding_end: u32) -> Self {
        self.padding_start = padding_start;
        self.padding_end = padding_end;
        self
    }

    pub fn with_scroll_padding(
        mut self,
        scroll_padding_start: u32,
        scroll_padding_end: u32,
    ) -> Self {
        self.scroll_padding_start = scroll_padding_start;
        self.scroll_padding_end = scroll_padding_end;
        self
    }

    pub fn with_scroll_margin(mut self, scroll_margin: u32) -> Self {
        self.scroll_margin = scroll_margin;
        self
    }

    pub fn with_initial_offset(mut self, initial_offset: InitialOffset) -> Self {
        self.initial_offset = initial_offset;
        self
    }

    pub fn with_initial_offset_value(mut self, initial_offset: u64) -> Self {
        self.initial_offset = InitialOffset::Value(initial_offset);
        self
    }

    pub fn with_initial_offset_provider(
        mut self,
        initial_offset: impl Fn() -> u64 + Send + Sync + 'static,
    ) -> Self {
        self.initial_offset = InitialOffset::Provider(Arc::new(initial_offset));
        self
    }

    pub fn with_on_change(
        mut self,
        on_change: Option<impl Fn(&Virtualizer<K>, bool) + Send + Sync + 'static>,
    ) -> Self {
        self.on_change = on_change.map(|f| Arc::new(f) as _);
        self
    }

    pub fn with_use_scrollend_event(mut self, use_scrollend_event: bool) -> Self {
        self.use_scrollend_event = use_scrollend_event;
        self
    }

    pub fn with_is_scrolling_reset_delay_ms(mut self, delay_ms: u64) -> Self {
        self.is_scrolling_reset_delay_ms = delay_ms;
        self
    }

    pub fn with_should_adjust_scroll_position_on_item_size_change(
        mut self,
        f: Option<impl Fn(&Virtualizer<K>, VirtualItem, i64) -> bool + Send + Sync + 'static>,
    ) -> Self {
        self.should_adjust_scroll_position_on_item_size_change = f.map(|f| Arc::new(f) as _);
        self
    }

    pub fn with_gap(mut self, gap: u32) -> Self {
        self.gap = gap;
        self
    }

    pub fn with_horizontal(mut self, horizontal: bool) -> Self {
        self.horizontal = horizontal;
        self
    }
}

impl<K> core::fmt::Debug for VirtualizerOptions<K> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("VirtualizerOptions")
            .field("count", &self.count)
            .field("enabled", &self.enabled)
            .field("debug", &self.debug)
            .field("overscan", &self.overscan)
            .field("initial_rect", &self.initial_rect)
            .field("padding_start", &self.padding_start)
            .field("padding_end", &self.padding_end)
            .field("scroll_padding_start", &self.scroll_padding_start)
            .field("scroll_padding_end", &self.scroll_padding_end)
            .field("scroll_margin", &self.scroll_margin)
            .field("initial_offset", &self.initial_offset)
            .field("use_scrollend_event", &self.use_scrollend_event)
            .field(
                "is_scrolling_reset_delay_ms",
                &self.is_scrolling_reset_delay_ms,
            )
            .field("gap", &self.gap)
            .field("horizontal", &self.horizontal)
            .finish_non_exhaustive()
    }
}

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
}

impl<K: KeyCacheKey> Virtualizer<K> {
    pub fn new(options: VirtualizerOptions<K>) -> Self {
        let scroll_rect = options.initial_rect.unwrap_or_default();
        let scroll_offset = options.initial_offset.resolve();
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
        self.options = options;

        if !self.options.enabled {
            self.viewport_size = 0;
            self.scroll_offset = self.options.initial_offset.resolve();
            self.scroll_rect = Rect::default();
            self.is_scrolling = false;
            self.scroll_direction = None;
            self.last_scroll_event_ms = None;
        } else if !was_enabled {
            self.reset_to_initial();
        } else if self.options.count != prev_count {
            self.rebuild_estimates();
        } else if self.options.gap != prev_gap {
            self.rebuild_fenwick();
        }

        self.notify();
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

    fn notify(&self) {
        if let Some(cb) = &self.options.on_change {
            cb(self, self.is_scrolling);
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

    pub fn debug(&self) -> bool {
        self.options.debug
    }

    pub fn set_debug(&mut self, debug: bool) {
        if self.options.debug == debug {
            return;
        }
        self.options.debug = debug;
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

    pub fn set_scroll_rect(&mut self, rect: Rect) {
        self.scroll_rect = rect;
        self.viewport_size = rect.main;
        self.notify();
    }

    pub fn scroll_offset(&self) -> u64 {
        self.scroll_offset
    }

    pub fn scroll_offset_in_list(&self) -> u64 {
        let margin = self.options.scroll_margin as u64;
        self.scroll_offset.saturating_sub(margin)
    }

    pub fn set_viewport_size(&mut self, size: u32) {
        self.viewport_size = size;
        self.scroll_rect.main = size;
        self.notify();
    }

    pub fn set_scroll_offset(&mut self, offset: u64) {
        let prev = self.scroll_offset;
        self.scroll_offset = offset;
        self.scroll_direction = match offset.cmp(&prev) {
            cmp::Ordering::Greater => Some(ScrollDirection::Forward),
            cmp::Ordering::Less => Some(ScrollDirection::Backward),
            cmp::Ordering::Equal => self.scroll_direction,
        };
        self.notify();
    }

    pub fn set_scroll_offset_clamped(&mut self, offset: u64) {
        let clamped = self.clamp_scroll_offset(offset);
        self.set_scroll_offset(clamped);
    }

    pub fn scroll_to_offset(&mut self, offset: u64) {
        self.set_scroll_offset(offset);
    }

    pub fn scroll_to_offset_clamped(&mut self, offset: u64) {
        self.set_scroll_offset_clamped(offset);
    }

    pub fn set_viewport_and_scroll(&mut self, viewport_size: u32, scroll_offset: u64) {
        self.viewport_size = viewport_size;
        self.scroll_rect.main = viewport_size;
        self.set_scroll_offset(scroll_offset);
    }

    pub fn set_viewport_and_scroll_clamped(&mut self, viewport_size: u32, scroll_offset: u64) {
        self.viewport_size = viewport_size;
        self.scroll_rect.main = viewport_size;
        self.set_scroll_offset_clamped(scroll_offset);
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

    pub fn set_horizontal(&mut self, horizontal: bool) {
        self.options.horizontal = horizontal;
        self.notify();
    }

    pub fn set_get_item_key(&mut self, f: impl Fn(usize) -> K + Send + Sync + 'static) {
        self.options.get_item_key = Arc::new(f);
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
        f: Option<impl Fn(VirtualRange) -> Vec<usize> + Send + Sync + 'static>,
    ) {
        self.options.range_extractor = f.map(|f| Arc::new(f) as _);
        self.notify();
    }

    pub fn set_range_extractor_v2(
        &mut self,
        f: Option<impl Fn(Range) -> Vec<usize> + Send + Sync + 'static>,
    ) {
        self.options.range_extractor_v2 = f.map(|f| Arc::new(f) as _);
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

    pub fn clear_measurement_cache(&mut self) {
        self.reset_measurements();
    }

    pub fn measure_all(&mut self) {
        self.reset_measurements();
    }

    pub fn is_measured(&self, index: usize) -> bool {
        self.measured.get(index).copied().unwrap_or(false)
    }

    pub fn get_total_size(&self) -> u64 {
        if !self.options.enabled {
            return 0;
        }
        self.options.padding_start as u64 + self.sums.total() + self.options.padding_end as u64
    }

    pub fn key_for(&self, index: usize) -> K {
        (self.options.get_item_key)(index)
    }

    pub fn get_virtual_range(&self) -> VirtualRange {
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

    pub fn get_visible_range(&self) -> VirtualRange {
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

    pub fn get_virtual_indexes(&self) -> Vec<usize> {
        self.virtual_indexes_for(self.scroll_offset, self.viewport_size)
    }

    pub fn virtual_indexes_for(&self, scroll_offset: u64, viewport_size: u32) -> Vec<usize> {
        if !self.options.enabled {
            return Vec::new();
        }

        let visible = self.visible_range_for(scroll_offset, viewport_size);
        if visible.is_empty() {
            return Vec::new();
        }

        let count = self.options.count;
        let overscan = self.options.overscan;
        let overscanned = VirtualRange {
            start_index: visible.start_index.saturating_sub(overscan),
            end_index: cmp::min(count, visible.end_index.saturating_add(overscan)),
        };

        if let Some(extract) = &self.options.range_extractor_v2 {
            return extract(Range {
                start_index: visible.start_index,
                end_index: visible.end_index,
                overscan,
                count,
            })
            .into_iter()
            .filter(|&i| i < count)
            .collect();
        }

        if let Some(extract) = &self.options.range_extractor {
            extract(overscanned)
                .into_iter()
                .filter(|&i| i < self.options.count)
                .collect()
        } else {
            (overscanned.start_index..overscanned.end_index).collect()
        }
    }

    pub fn get_virtual_items(&self) -> Vec<VirtualItem> {
        if !self.options.enabled {
            return Vec::new();
        }
        if self.options.range_extractor_v2.is_some() {
            let idxs = self.get_virtual_indexes();
            let mut out: Vec<VirtualItem> = Vec::with_capacity(idxs.len());
            for i in idxs {
                if i >= self.options.count {
                    continue;
                }
                out.push(self.item(i));
            }
            return out;
        }
        let range = self.get_virtual_range();
        self.virtual_items_for_range(range)
    }

    pub fn virtual_items_for(&self, scroll_offset: u64, viewport_size: u32) -> Vec<VirtualItem> {
        let range = self.virtual_range_for(scroll_offset, viewport_size);
        self.virtual_items_for_range(range)
    }

    pub fn get_virtual_items_keyed(&self) -> Vec<VirtualItemKeyed<K>> {
        if !self.options.enabled {
            return Vec::new();
        }
        if self.options.range_extractor_v2.is_some() {
            let idxs = self.get_virtual_indexes();
            let mut out: Vec<VirtualItemKeyed<K>> = Vec::with_capacity(idxs.len());
            for i in idxs {
                if i >= self.options.count {
                    continue;
                }
                let item = self.item(i);
                out.push(VirtualItemKeyed {
                    key: self.key_for(i),
                    index: item.index,
                    start: item.start,
                    size: item.size,
                });
            }
            return out;
        }
        let range = self.get_virtual_range();
        self.virtual_items_keyed_for_range(range)
    }

    pub fn virtual_items_keyed_for(
        &self,
        scroll_offset: u64,
        viewport_size: u32,
    ) -> Vec<VirtualItemKeyed<K>> {
        let range = self.virtual_range_for(scroll_offset, viewport_size);
        self.virtual_items_keyed_for_range(range)
    }

    fn virtual_items_for_range(&self, range: VirtualRange) -> Vec<VirtualItem> {
        if range.is_empty() {
            return Vec::new();
        }

        if let Some(extract) = &self.options.range_extractor {
            let idxs = extract(range);
            let mut out: Vec<VirtualItem> = Vec::with_capacity(idxs.len());
            for i in idxs {
                if i >= self.options.count {
                    continue;
                }
                out.push(self.item(i));
            }
            out
        } else {
            // Fast path for contiguous ranges: compute the first start offset once and then walk.
            let cap = range.end_index.saturating_sub(range.start_index);
            let mut out: Vec<VirtualItem> = Vec::with_capacity(cap);

            let margin = self.options.scroll_margin as u64;
            let mut start = margin.saturating_add(self.start_of(range.start_index));
            let gap = self.options.gap as u64;
            let count = self.options.count;

            for i in range.start_index..range.end_index {
                let size = self.sizes[i];
                out.push(VirtualItem {
                    index: i,
                    start,
                    size,
                });

                start = start.saturating_add(size as u64);
                if gap > 0 && i + 1 < count {
                    start = start.saturating_add(gap);
                }
            }

            out
        }
    }

    fn virtual_items_keyed_for_range(&self, range: VirtualRange) -> Vec<VirtualItemKeyed<K>> {
        if range.is_empty() {
            return Vec::new();
        }

        if let Some(extract) = &self.options.range_extractor {
            let idxs = extract(range);
            let mut out: Vec<VirtualItemKeyed<K>> = Vec::with_capacity(idxs.len());
            for i in idxs {
                if i >= self.options.count {
                    continue;
                }
                let item = self.item(i);
                out.push(VirtualItemKeyed {
                    key: self.key_for(i),
                    index: item.index,
                    start: item.start,
                    size: item.size,
                });
            }
            out
        } else {
            let cap = range.end_index.saturating_sub(range.start_index);
            let mut out: Vec<VirtualItemKeyed<K>> = Vec::with_capacity(cap);

            let margin = self.options.scroll_margin as u64;
            let mut start = margin.saturating_add(self.start_of(range.start_index));
            let gap = self.options.gap as u64;
            let count = self.options.count;

            for i in range.start_index..range.end_index {
                let size = self.sizes[i];
                out.push(VirtualItemKeyed {
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

            out
        }
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

    pub fn scroll_to_index(&mut self, index: usize, align: Align) {
        let off = self.scroll_to_index_offset(index, align);
        self.set_scroll_offset(off);
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
        let count = self.options.count;
        let gap = self.options.gap as u64;
        let mut effective: Vec<u64> = Vec::with_capacity(count);
        for i in 0..count {
            let mut s = self.sizes[i] as u64;
            if gap > 0 && i + 1 < count {
                s = s.saturating_add(gap);
            }
            effective.push(s);
        }
        self.sums = Fenwick::from_values(effective);
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
        let total = self.get_total_size();
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

        let scroll_end = scroll_offset.saturating_add(view);
        if scroll_end <= margin {
            return VirtualRange {
                start_index: 0,
                end_index: 0,
            };
        }

        let off = scroll_offset.saturating_sub(margin);
        let visible_end_exclusive = scroll_end.saturating_sub(margin);

        let total = self.get_total_size();
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

#[derive(Clone, Debug)]
struct Fenwick {
    tree: Vec<u64>, // 1-indexed
}

impl Fenwick {
    fn new(n: usize) -> Self {
        Self {
            tree: alloc::vec![0; n + 1],
        }
    }

    fn from_values(values: Vec<u64>) -> Self {
        let n = values.len();
        let mut tree = alloc::vec![0u64; n + 1];
        for i in 1..=n {
            tree[i] = tree[i].saturating_add(values[i - 1]);
            let j = i + lsb(i);
            if j <= n {
                tree[j] = tree[j].saturating_add(tree[i]);
            }
        }
        Self { tree }
    }

    fn len(&self) -> usize {
        self.tree.len().saturating_sub(1)
    }

    fn add(&mut self, index: usize, delta: i64) {
        let n = self.len();
        if index >= n {
            return;
        }
        let mut i = index + 1;
        while i <= n {
            let cur = self.tree[i] as i128;
            let next = cur + delta as i128;
            debug_assert!(
                next >= 0,
                "Fenwick underflow (idx={i}, cur={cur}, delta={delta})"
            );
            self.tree[i] = next.clamp(0, u64::MAX as i128) as u64;
            i += lsb(i);
        }
    }

    fn prefix_sum(&self, count: usize) -> u64 {
        let n = self.len();
        let mut i = cmp::min(count, n);
        let mut sum = 0u64;
        while i > 0 {
            sum = sum.saturating_add(self.tree[i]);
            i &= i - 1;
        }
        sum
    }

    fn total(&self) -> u64 {
        self.prefix_sum(self.len())
    }

    /// Returns the number of items whose prefix sum is <= `target`.
    ///
    /// This is useful to map an offset to an index:
    /// - `index = lower_bound(offset)` returns the item index at `offset` (clamped).
    fn lower_bound(&self, mut target: u64) -> usize {
        let n = self.len();
        if n == 0 {
            return 0;
        }

        let mut idx = 0usize;
        let mut bit = highest_power_of_two_leq(n);
        while bit != 0 {
            let next = idx + bit;
            if next <= n && self.tree[next] <= target {
                target -= self.tree[next];
                idx = next;
            }
            bit >>= 1;
        }
        idx
    }
}

fn lsb(i: usize) -> usize {
    i & i.wrapping_neg()
}

fn highest_power_of_two_leq(n: usize) -> usize {
    let mut p = 1usize;
    while p <= n / 2 {
        p <<= 1;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU64, Ordering};

    static INITIAL_OFFSET_PROVIDER_CALLED: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn fixed_size_range_and_total() {
        let mut v = Virtualizer::new(VirtualizerOptions::new(100, |_| 1));
        v.set_viewport_size(10);
        v.set_scroll_offset(0);
        assert_eq!(v.get_total_size(), 100);

        let r = v.get_virtual_range();
        assert_eq!(r.start_index, 0);
        // 10 visible + overscan(1) at end
        assert_eq!(r.end_index, 11);
    }

    #[test]
    fn overscan_and_scroll() {
        let mut v = Virtualizer::new(VirtualizerOptions::new(100, |_| 1));
        v.set_viewport_size(10);
        v.set_scroll_offset(50);
        let r = v.get_virtual_range();
        assert_eq!(r.start_index, 49);
        assert_eq!(r.end_index, 61);
    }

    #[test]
    fn padding_and_gap_affect_total_and_positions() {
        let mut opts = VirtualizerOptions::new(3, |_| 2);
        opts.padding_start = 10;
        opts.padding_end = 5;
        opts.gap = 1;
        let v = Virtualizer::new(opts);
        // total = pad_start(10) + effective sizes((2+1)+(2+1)+2=8) + pad_end(5) = 23
        assert_eq!(v.get_total_size(), 23);

        let i0 = v.get_virtual_items(); // viewport size 0 => empty
        assert!(i0.is_empty());
    }

    #[test]
    fn measure_updates_total_and_scroll_to_index() {
        let mut opts = VirtualizerOptions::new(5, |_| 1);
        opts.scroll_padding_start = 2;
        let mut v = Virtualizer::new(opts);
        v.set_viewport_size(3);

        assert_eq!(v.get_total_size(), 5);
        v.measure(2, 10);
        assert_eq!(v.get_total_size(), 14);

        // item 2 starts at 2 (sizes 1+1)
        assert_eq!(v.scroll_to_index_offset(2, Align::Start), 0); // start(2)=2, minus sp(2) => 0
        assert_eq!(v.scroll_to_index_offset(4, Align::End), 11); // end(4)=14, view=3 => 11
    }

    #[test]
    fn index_at_offset_with_gap_maps_into_previous_item() {
        let mut opts = VirtualizerOptions::new(2, |_| 2);
        opts.gap = 1; // layout: item0(0..2), gap(2..3), item1(3..5)
        let v = Virtualizer::new(opts);
        assert_eq!(v.index_at_offset(0), Some(0));
        assert_eq!(v.index_at_offset(1), Some(0));
        assert_eq!(v.index_at_offset(2), Some(0)); // inside gap treated as previous
        assert_eq!(v.index_at_offset(3), Some(1));
        assert_eq!(v.index_at_offset(4), Some(1));
    }

    #[test]
    fn range_extractor_can_pin_indices() {
        let mut opts = VirtualizerOptions::new(100, |_| 1);
        opts.overscan = 0;
        opts.range_extractor = Some(Arc::new(|r: VirtualRange| {
            let mut v: Vec<usize> = (r.start_index..r.end_index).collect();
            v.push(0); // pin header
            v.sort_unstable();
            v.dedup();
            v
        }));
        let mut v = Virtualizer::new(opts);
        v.set_viewport_size(5);
        v.set_scroll_offset(50);
        let items = v.get_virtual_items();
        assert!(items.iter().any(|it| it.index == 0));
        assert!(items.iter().any(|it| it.index == 50));
    }

    #[test]
    fn measurements_follow_keys_after_reorder() {
        let mut v = Virtualizer::new(VirtualizerOptions::new(2, |_| 1));
        v.measure(0, 10);
        assert_eq!(v.item_size(0), Some(10));
        assert_eq!(v.item_size(1), Some(1));

        // Simulate data reorder by changing the key mapping.
        v.set_get_item_key(|i| if i == 0 { 1 } else { 0 });
        v.sync_item_keys();

        // The measured size (10) should follow key=0, now at index 1.
        assert_eq!(v.item_size(0), Some(1));
        assert_eq!(v.item_size(1), Some(10));
    }

    #[test]
    fn scroll_margin_affects_visibility_and_item_starts() {
        let mut opts = VirtualizerOptions::new(100, |_| 1);
        opts.scroll_margin = 50;
        let mut v = Virtualizer::new(opts);
        v.set_viewport_size(10);

        // Viewport ends before the list starts.
        v.set_scroll_offset(0);
        assert!(v.get_virtual_items().is_empty());

        // Viewport overlaps the list.
        v.set_scroll_offset(45);
        let items = v.get_virtual_items();
        assert!(!items.is_empty());
        assert_eq!(items[0].index, 0);
        assert_eq!(items[0].start, 50);
    }

    #[test]
    fn resize_item_can_adjust_scroll_to_prevent_jumps() {
        let mut v = Virtualizer::new(VirtualizerOptions::new(5, |_| 10));
        v.set_viewport_size(10);
        v.set_scroll_offset(30);

        // Item 0 starts before scroll offset, so resizing it should shift the scroll position.
        let applied = v.resize_item(0, 15);
        assert_eq!(applied, 5);
        assert_eq!(v.scroll_offset(), 35);
    }

    #[test]
    fn virtual_item_for_offset_maps_to_correct_index() {
        let mut opts = VirtualizerOptions::new(2, |_| 2);
        opts.gap = 1; // item0(0..2), gap(2..3), item1(3..5)
        let v = Virtualizer::new(opts);
        assert_eq!(v.virtual_item_for_offset(0).unwrap().index, 0);
        assert_eq!(v.virtual_item_for_offset(2).unwrap().index, 0);
        assert_eq!(v.virtual_item_for_offset(3).unwrap().index, 1);
    }

    #[test]
    fn measure_all_clears_measurements() {
        let mut v = Virtualizer::new(VirtualizerOptions::new(3, |_| 1));
        v.measure(1, 10);
        assert!(v.is_measured(1));
        assert_eq!(v.item_size(1), Some(10));

        v.measure_all();
        assert!(!v.is_measured(1));
        assert_eq!(v.item_size(1), Some(1));
    }

    #[test]
    fn scroll_to_offset_clamped_respects_max_scroll_offset() {
        let mut v = Virtualizer::new(VirtualizerOptions::new(10, |_| 1));
        v.set_viewport_size(3);
        let max = v.max_scroll_offset();
        v.scroll_to_offset_clamped(u64::MAX);
        assert_eq!(v.scroll_offset(), max);
    }

    #[test]
    fn is_scrolling_resets_after_delay_without_scrollend_event() {
        let mut opts = VirtualizerOptions::new(10, |_| 1);
        opts.is_scrolling_reset_delay_ms = 10;
        let mut v = Virtualizer::new(opts);
        v.notify_scroll_event(0);
        assert!(v.is_scrolling());
        v.update_scrolling(9);
        assert!(v.is_scrolling());
        v.update_scrolling(10);
        assert!(!v.is_scrolling());
    }

    #[test]
    fn range_extractor_v2_receives_visible_range_and_overscan() {
        let mut opts =
            VirtualizerOptions::new(100, |_| 1).with_range_extractor_v2(Some(|r: Range| {
                assert_eq!(r.overscan, 1);
                let mut out: Vec<usize> = (r.start_index..r.end_index).collect();
                out.push(0);
                out
            }));
        opts.overscan = 1;
        let mut v = Virtualizer::new(opts);
        v.set_viewport_size(5);
        v.set_scroll_offset(50);
        let idxs = v.get_virtual_indexes();
        assert!(idxs.contains(&0));
        assert!(idxs.contains(&50));
    }

    #[test]
    fn initial_rect_sets_viewport_and_scroll_rect() {
        let opts = VirtualizerOptions::new(1, |_| 1).with_initial_rect(Some(Rect {
            main: 10,
            cross: 20,
        }));
        let v = Virtualizer::new(opts);
        assert_eq!(v.viewport_size(), 10);
        assert_eq!(
            v.scroll_rect(),
            Rect {
                main: 10,
                cross: 20
            }
        );
    }

    #[test]
    fn set_scroll_rect_updates_viewport_size() {
        let mut v = Virtualizer::new(VirtualizerOptions::new(1, |_| 1));
        v.set_scroll_rect(Rect { main: 7, cross: 9 });
        assert_eq!(v.viewport_size(), 7);
        assert_eq!(v.scroll_rect(), Rect { main: 7, cross: 9 });
    }

    #[test]
    fn initial_offset_provider_is_used() {
        INITIAL_OFFSET_PROVIDER_CALLED.store(0, Ordering::Relaxed);
        let opts = VirtualizerOptions::new(1, |_| 1).with_initial_offset(InitialOffset::Provider(
            Arc::new(|| {
                INITIAL_OFFSET_PROVIDER_CALLED.fetch_add(1, Ordering::Relaxed);
                42
            }),
        ));
        let v = Virtualizer::new(opts);
        assert_eq!(v.scroll_offset(), 42);
        assert!(INITIAL_OFFSET_PROVIDER_CALLED.load(Ordering::Relaxed) >= 1);
    }
}
