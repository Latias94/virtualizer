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
#![forbid(unsafe_code)]

use std::cmp;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    Auto,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualItem {
    pub index: usize,
    /// Start offset in the scroll axis (includes `padding_start`).
    pub start: u64,
    /// Size in the scroll axis (excludes `gap`).
    pub size: u32,
}

impl VirtualItem {
    pub fn end(&self) -> u64 {
        self.start + self.size as u64
    }
}

pub type ItemKey = u64;

#[derive(Clone)]
pub struct VirtualizerOptions {
    pub count: usize,
    pub estimate_size: Arc<dyn Fn(usize) -> u32 + Send + Sync>,
    pub get_item_key: Arc<dyn Fn(usize) -> ItemKey + Send + Sync>,
    pub range_extractor: Option<Arc<dyn Fn(VirtualRange) -> Vec<usize> + Send + Sync>>,

    pub overscan: usize,

    /// Padding before the first item.
    pub padding_start: u32,
    /// Padding after the last item.
    pub padding_end: u32,

    /// Additional padding applied when computing scroll-to offsets.
    pub scroll_padding_start: u32,
    /// Additional padding applied when computing scroll-to offsets.
    pub scroll_padding_end: u32,

    /// Space between items.
    pub gap: u32,

    /// Orientation hint (does not change math; kept for parity with TanStack Virtual).
    pub horizontal: bool,
}

impl VirtualizerOptions {
    pub fn new(count: usize, estimate_size: impl Fn(usize) -> u32 + Send + Sync + 'static) -> Self {
        Self {
            count,
            estimate_size: Arc::new(estimate_size),
            get_item_key: Arc::new(|i| i as u64),
            range_extractor: None,
            overscan: 1,
            padding_start: 0,
            padding_end: 0,
            scroll_padding_start: 0,
            scroll_padding_end: 0,
            gap: 0,
            horizontal: false,
        }
    }
}

impl std::fmt::Debug for VirtualizerOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualizerOptions")
            .field("count", &self.count)
            .field("overscan", &self.overscan)
            .field("padding_start", &self.padding_start)
            .field("padding_end", &self.padding_end)
            .field("scroll_padding_start", &self.scroll_padding_start)
            .field("scroll_padding_end", &self.scroll_padding_end)
            .field("gap", &self.gap)
            .field("horizontal", &self.horizontal)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub struct Virtualizer {
    options: VirtualizerOptions,
    viewport_size: u32,
    scroll_offset: u64,

    sizes: Vec<u32>, // base sizes (no gap)
    measured: Vec<bool>,
    sums: Fenwick,
}

impl Virtualizer {
    pub fn new(options: VirtualizerOptions) -> Self {
        let mut v = Self {
            viewport_size: 0,
            scroll_offset: 0,
            sizes: Vec::new(),
            measured: Vec::new(),
            sums: Fenwick::new(0),
            options,
        };
        v.rebuild_estimates();
        v
    }

    pub fn options(&self) -> &VirtualizerOptions {
        &self.options
    }

    pub fn count(&self) -> usize {
        self.options.count
    }

    pub fn viewport_size(&self) -> u32 {
        self.viewport_size
    }

    pub fn scroll_offset(&self) -> u64 {
        self.scroll_offset
    }

    pub fn set_viewport_size(&mut self, size: u32) {
        self.viewport_size = size;
        self.scroll_offset = self.clamp_scroll(self.scroll_offset);
    }

    pub fn set_scroll_offset(&mut self, offset: u64) {
        self.scroll_offset = self.clamp_scroll(offset);
    }

    pub fn set_count(&mut self, count: usize) {
        if self.options.count == count {
            return;
        }
        self.options.count = count;
        self.rebuild_estimates();
        self.scroll_offset = self.clamp_scroll(self.scroll_offset);
    }

    pub fn set_overscan(&mut self, overscan: usize) {
        self.options.overscan = overscan;
    }

    pub fn set_padding(&mut self, padding_start: u32, padding_end: u32) {
        self.options.padding_start = padding_start;
        self.options.padding_end = padding_end;
        self.scroll_offset = self.clamp_scroll(self.scroll_offset);
    }

    pub fn set_scroll_padding(&mut self, scroll_padding_start: u32, scroll_padding_end: u32) {
        self.options.scroll_padding_start = scroll_padding_start;
        self.options.scroll_padding_end = scroll_padding_end;
        self.scroll_offset = self.clamp_scroll(self.scroll_offset);
    }

    pub fn set_gap(&mut self, gap: u32) {
        if self.options.gap == gap {
            return;
        }
        self.options.gap = gap;
        self.rebuild_fenwick();
        self.scroll_offset = self.clamp_scroll(self.scroll_offset);
    }

    pub fn set_horizontal(&mut self, horizontal: bool) {
        self.options.horizontal = horizontal;
    }

    pub fn set_get_item_key(&mut self, f: impl Fn(usize) -> ItemKey + Send + Sync + 'static) {
        self.options.get_item_key = Arc::new(f);
    }

    pub fn set_range_extractor(
        &mut self,
        f: Option<impl Fn(VirtualRange) -> Vec<usize> + Send + Sync + 'static>,
    ) {
        self.options.range_extractor = f.map(|f| Arc::new(f) as _);
    }

    pub fn set_estimate_size(&mut self, f: impl Fn(usize) -> u32 + Send + Sync + 'static) {
        self.options.estimate_size = Arc::new(f);
        self.rebuild_estimates();
        self.scroll_offset = self.clamp_scroll(self.scroll_offset);
    }

    pub fn reset_measurements(&mut self) {
        for (i, was_measured) in self.measured.iter_mut().enumerate() {
            if *was_measured {
                *was_measured = false;
                let est = (self.options.estimate_size)(i);
                let cur = self.sizes[i];
                if cur != est {
                    self.sizes[i] = est;
                }
            }
        }
        self.rebuild_fenwick();
        self.scroll_offset = self.clamp_scroll(self.scroll_offset);
    }

    pub fn measure(&mut self, index: usize, size: u32) {
        if index >= self.options.count {
            return;
        }
        let cur = self.sizes[index];
        if cur == size {
            self.measured[index] = true;
            return;
        }
        self.sizes[index] = size;
        self.measured[index] = true;
        self.sums.add(index, size as i64 - cur as i64);
        self.scroll_offset = self.clamp_scroll(self.scroll_offset);
    }

    pub fn is_measured(&self, index: usize) -> bool {
        self.measured.get(index).copied().unwrap_or(false)
    }

    pub fn get_total_size(&self) -> u64 {
        self.options.padding_start as u64 + self.sums.total() + self.options.padding_end as u64
    }

    pub fn key_for(&self, index: usize) -> ItemKey {
        (self.options.get_item_key)(index)
    }

    pub fn get_virtual_range(&self) -> VirtualRange {
        self.compute_range(self.scroll_offset, self.viewport_size)
    }

    pub fn get_virtual_items(&self) -> Vec<VirtualItem> {
        let range = self.get_virtual_range();
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
            let mut out: Vec<VirtualItem> =
                Vec::with_capacity(range.end_index.saturating_sub(range.start_index));
            for i in range.start_index..range.end_index {
                out.push(self.item(i));
            }
            out
        }
    }

    pub fn scroll_to_index_offset(&self, index: usize, align: Align) -> u64 {
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
                let center = item.start + (item.size as u64 / 2);
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

        self.clamp_scroll(target)
    }

    pub fn scroll_to_index(&mut self, index: usize, align: Align) {
        let off = self.scroll_to_index_offset(index, align);
        self.set_scroll_offset(off);
    }

    pub fn index_at_offset(&self, offset: u64) -> Option<usize> {
        self.index_at_offset_inner(offset)
            .filter(|&i| i < self.options.count)
    }

    fn rebuild_estimates(&mut self) {
        self.sizes.clear();
        self.measured.clear();
        self.sizes
            .reserve_exact(self.options.count.saturating_sub(self.sizes.len()));
        self.measured
            .reserve_exact(self.options.count.saturating_sub(self.measured.len()));

        for i in 0..self.options.count {
            self.sizes.push((self.options.estimate_size)(i));
            self.measured.push(false);
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
        let start = self.start_of(index);
        VirtualItem {
            index,
            start,
            size: self.sizes[index],
        }
    }

    fn start_of(&self, index: usize) -> u64 {
        self.options.padding_start as u64 + self.sums.prefix_sum(index)
    }

    fn clamp_scroll(&self, offset: u64) -> u64 {
        let total = self.get_total_size();
        let view = self.viewport_size as u64;
        if total <= view {
            return 0;
        }
        offset.min(total - view)
    }

    fn compute_range(&self, scroll_offset: u64, viewport_size: u32) -> VirtualRange {
        let count = self.options.count;
        if count == 0 || viewport_size == 0 {
            return VirtualRange {
                start_index: 0,
                end_index: 0,
            };
        }

        let off = scroll_offset;
        let total = self.get_total_size();
        if off >= total {
            return VirtualRange {
                start_index: count,
                end_index: count,
            };
        }

        let view = viewport_size as u64;
        let visible_start = off;
        let visible_end_exclusive = off.saturating_add(view);
        let visible_end_inclusive = visible_end_exclusive.saturating_sub(1);

        let mut start = self.index_at_offset_inner(visible_start).unwrap_or(count);
        let mut end = self
            .index_at_offset_inner(cmp::max(visible_end_inclusive, visible_start))
            .map(|i| i + 1)
            .unwrap_or(count);

        start = start.saturating_sub(self.options.overscan);
        end = cmp::min(count, end.saturating_add(self.options.overscan));

        VirtualRange {
            start_index: start,
            end_index: end,
        }
    }

    fn index_at_offset_inner(&self, offset: u64) -> Option<usize> {
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
        Self { tree: vec![0; n + 1] }
    }

    fn from_values(values: Vec<u64>) -> Self {
        let n = values.len();
        let mut tree = vec![0u64; n + 1];
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
            let cur = self.tree[i] as i64;
            let next = cur + delta;
            debug_assert!(next >= 0, "Fenwick underflow (idx={i}, cur={cur}, delta={delta})");
            self.tree[i] = next.max(0) as u64;
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
    i & (!i + 1)
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
}
