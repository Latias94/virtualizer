use alloc::sync::Arc;

use crate::virtualizer::Virtualizer;
use crate::{ItemKey, Range, Rect, VirtualItem};

/// A callback fired when a virtualizer state update occurs.
///
/// The second argument is `is_scrolling`.
pub type OnChangeCallback<K> = Arc<dyn Fn(&Virtualizer<K>, bool) + Send + Sync>;

/// A hook that decides whether to adjust scroll position when an item size changes.
///
/// This is typically used to prevent visual "jumps" when an item above the current scroll offset
/// is measured and differs from its estimate.
pub type ShouldAdjustScrollPositionOnItemSizeChangeCallback<K> =
    Arc<dyn Fn(&Virtualizer<K>, VirtualItem, i64) -> bool + Send + Sync>;

/// A callback that emits virtual item indexes for a given visible range.
///
/// This is designed for zero-allocation adapters: instead of returning a `Vec`, the extractor
/// receives an `emit` callback and can push indexes directly into the adapter's output buffer.
///
/// Contract:
/// - `emit(i)` must be called with `i < range.count`.
/// - The emitted indexes must be sorted ascending; duplicates are allowed but ignored.
///
/// Tip: use [`crate::IndexEmitter`] to enforce the contract (and to avoid accidental panics in
/// debug builds).
pub type RangeExtractor = Arc<dyn Fn(Range, &mut dyn FnMut(usize)) + Send + Sync>;

/// Initial scroll offset configuration.
#[derive(Clone)]
pub enum InitialOffset {
    /// A fixed initial offset.
    Value(u64),
    /// A lazily evaluated initial offset provider (called by `Virtualizer::new`).
    Provider(Arc<dyn Fn() -> u64 + Send + Sync>),
}

impl InitialOffset {
    pub(crate) fn resolve(&self) -> u64 {
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

/// Configuration for [`crate::Virtualizer`].
///
/// This type is designed to be cheap to clone: heavy fields are stored in `Arc`s so adapters can
/// update a few fields and call `Virtualizer::set_options` without reallocating closures.
pub struct VirtualizerOptions<K = ItemKey> {
    pub count: usize,
    pub estimate_size: Arc<dyn Fn(usize) -> u32 + Send + Sync>,
    pub get_item_key: Arc<dyn Fn(usize) -> K + Send + Sync>,
    /// Optional index selection hook.
    ///
    /// When set, the virtualizer will call this extractor to emit the final set of indexes to
    /// render. This is useful for pinned/sticky rows, section headers, etc.
    ///
    /// The extractor receives the *visible* range (no overscan) plus `overscan` and `count`, and
    /// must emit a sorted (ascending) sequence of indexes. Duplicates are allowed but ignored.
    pub range_extractor: Option<RangeExtractor>,

    /// Enables/disables the virtualizer. When disabled, query methods return empty results.
    pub enabled: bool,

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
}

impl<K> Clone for VirtualizerOptions<K> {
    fn clone(&self) -> Self {
        Self {
            count: self.count,
            estimate_size: Arc::clone(&self.estimate_size),
            get_item_key: Arc::clone(&self.get_item_key),
            range_extractor: self.range_extractor.clone(),
            enabled: self.enabled,
            overscan: self.overscan,
            initial_rect: self.initial_rect,
            padding_start: self.padding_start,
            padding_end: self.padding_end,
            scroll_padding_start: self.scroll_padding_start,
            scroll_padding_end: self.scroll_padding_end,
            scroll_margin: self.scroll_margin,
            initial_offset: self.initial_offset.clone(),
            on_change: self.on_change.clone(),
            use_scrollend_event: self.use_scrollend_event,
            is_scrolling_reset_delay_ms: self.is_scrolling_reset_delay_ms,
            should_adjust_scroll_position_on_item_size_change: self
                .should_adjust_scroll_position_on_item_size_change
                .clone(),
            gap: self.gap,
        }
    }
}

impl VirtualizerOptions<ItemKey> {
    /// Creates options for a list keyed by index (`ItemKey = u64`).
    ///
    /// `estimate_size(i)` should return the estimated item size in the scroll axis (e.g. row
    /// height for vertical lists). The estimate is used until an item is measured.
    pub fn new(count: usize, estimate_size: impl Fn(usize) -> u32 + Send + Sync + 'static) -> Self {
        Self {
            count,
            estimate_size: Arc::new(estimate_size),
            get_item_key: Arc::new(|i| i as u64),
            range_extractor: None,
            enabled: true,
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
        }
    }
}

impl<K> VirtualizerOptions<K> {
    /// Creates options with a custom key mapping.
    ///
    /// Use this when you want measurements to follow items across reordering/replacement:
    /// `get_item_key(i)` should return a stable identity for the item at index `i`.
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
            enabled: true,
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
        range_extractor: Option<impl Fn(Range, &mut dyn FnMut(usize)) + Send + Sync + 'static>,
    ) -> Self {
        self.range_extractor = range_extractor.map(|f| Arc::new(f) as _);
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets the initial viewport rectangle.
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
}

impl<K> core::fmt::Debug for VirtualizerOptions<K> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("VirtualizerOptions")
            .field("count", &self.count)
            .field("enabled", &self.enabled)
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
            .finish_non_exhaustive()
    }
}
