use crate::*;

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::{AtomicU64, Ordering};

static INITIAL_OFFSET_PROVIDER_CALLED: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug)]
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        // Deterministic, dependency-free PRNG for tests.
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    fn gen_range_u64(&mut self, start: u64, end_exclusive: u64) -> u64 {
        debug_assert!(start < end_exclusive);
        let span = end_exclusive - start;
        start + (self.next_u64() % span)
    }

    fn gen_range_usize(&mut self, start: usize, end_exclusive: usize) -> usize {
        self.gen_range_u64(start as u64, end_exclusive as u64) as usize
    }

    fn gen_range_u32(&mut self, start: u32, end_exclusive: u32) -> u32 {
        self.gen_range_u64(start as u64, end_exclusive as u64) as u32
    }

    fn gen_bool(&mut self) -> bool {
        (self.next_u64() & 1) == 1
    }
}

fn expected_item_start_in_list(sizes: &[u32], gap: u32, padding_start: u32, index: usize) -> u64 {
    let mut off = padding_start as u64;
    for i in 0..index {
        off = off.saturating_add(sizes[i] as u64);
        if gap > 0 && i + 1 < sizes.len() {
            off = off.saturating_add(gap as u64);
        }
    }
    off
}

fn expected_total_size(sizes: &[u32], gap: u32, padding_start: u32, padding_end: u32) -> u64 {
    let mut total = padding_start as u64 + padding_end as u64;
    for (i, &sz) in sizes.iter().enumerate() {
        total = total.saturating_add(sz as u64);
        if gap > 0 && i + 1 < sizes.len() {
            total = total.saturating_add(gap as u64);
        }
    }
    total
}

fn expected_index_at_offset_in_list(
    sizes: &[u32],
    gap: u32,
    padding_start: u32,
    offset_in_list: u64,
) -> Option<usize> {
    let count = sizes.len();
    if count == 0 {
        return None;
    }
    let ps = padding_start as u64;
    if offset_in_list < ps {
        return Some(0);
    }
    let target = offset_in_list.saturating_sub(ps);

    // Match Fenwick::lower_bound semantics: return the largest `consumed` such that
    // prefix_sum(consumed) <= target, then clamp to a valid item index.
    let mut consumed = 0usize;
    let mut prefix = 0u64;
    for (i, &size) in sizes.iter().enumerate() {
        let mut seg = size as u64;
        if gap > 0 && i + 1 < count {
            seg = seg.saturating_add(gap as u64);
        }
        if prefix.saturating_add(seg) <= target {
            prefix = prefix.saturating_add(seg);
            consumed = consumed.saturating_add(1);
        } else {
            break;
        }
    }

    Some(consumed.min(count.saturating_sub(1)))
}

fn expected_visible_range(
    sizes: &[u32],
    gap: u32,
    padding_start: u32,
    padding_end: u32,
    scroll_margin: u32,
    scroll_offset: u64,
    viewport_size: u32,
) -> VirtualRange {
    let count = sizes.len();
    if count == 0 || viewport_size == 0 {
        return VirtualRange {
            start_index: 0,
            end_index: 0,
        };
    }

    let margin = scroll_margin as u64;
    let view = viewport_size as u64;
    let total = expected_total_size(sizes, gap, padding_start, padding_end);

    let max_scroll = margin.saturating_add(total.saturating_sub(view));
    let scroll_offset = scroll_offset.min(max_scroll);
    let scroll_end = scroll_offset.saturating_add(view);
    if scroll_end <= margin {
        return VirtualRange {
            start_index: 0,
            end_index: 0,
        };
    }

    let visible_start = scroll_offset.saturating_sub(margin);
    let visible_end_exclusive = scroll_end.saturating_sub(margin);
    if visible_start >= total {
        return VirtualRange {
            start_index: count,
            end_index: count,
        };
    }

    let visible_end_inclusive = visible_end_exclusive.saturating_sub(1);
    let start = expected_index_at_offset_in_list(sizes, gap, padding_start, visible_start)
        .unwrap_or(count)
        .min(count);
    let end = expected_index_at_offset_in_list(
        sizes,
        gap,
        padding_start,
        core::cmp::max(visible_end_inclusive, visible_start),
    )
    .map(|i| i + 1)
    .unwrap_or(count)
    .min(count);

    VirtualRange {
        start_index: start,
        end_index: end,
    }
}

#[test]
fn fixed_size_range_and_total() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(100, |_| 1));
    v.set_viewport_size(10);
    v.set_scroll_offset(0);
    assert_eq!(v.total_size(), 100);

    let r = v.virtual_range();
    assert_eq!(r.start_index, 0);
    // 10 visible + overscan(1) at end
    assert_eq!(r.end_index, 11);
}

#[test]
fn overscan_and_scroll() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(100, |_| 1));
    v.set_viewport_size(10);
    v.set_scroll_offset(50);
    let r = v.virtual_range();
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
    assert_eq!(v.total_size(), 23);

    let mut items = Vec::new();
    v.for_each_virtual_item(|it| items.push(it)); // viewport size 0 => empty
    assert!(items.is_empty());
}

#[test]
fn measure_updates_total_and_scroll_to_index_offset() {
    let mut opts = VirtualizerOptions::new(5, |_| 1);
    opts.scroll_padding_start = 2;
    let mut v = Virtualizer::new(opts);
    v.set_viewport_size(3);

    assert_eq!(v.total_size(), 5);
    v.measure(2, 10);
    assert_eq!(v.total_size(), 14);

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
    opts.range_extractor = Some(Arc::new(|r: Range, emit: &mut dyn FnMut(usize)| {
        let mut e = IndexEmitter::new(r, emit);
        e.emit_pinned(0); // pin header
        e.emit_visible();
    }));
    let mut v = Virtualizer::new(opts);
    v.set_viewport_size(5);
    v.set_scroll_offset(50);
    let mut items = Vec::new();
    v.for_each_virtual_item(|it| items.push(it));
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

    // The measured size (10) should follow key=0, now at index 1.
    assert_eq!(v.item_size(0), Some(1));
    assert_eq!(v.item_size(1), Some(10));
}

#[test]
fn set_count_updates_gap_bookkeeping() {
    let mut opts = VirtualizerOptions::new(1, |_| 2);
    opts.gap = 1;
    let mut v = Virtualizer::new(opts);

    // With a single item, there is no trailing gap.
    assert_eq!(v.total_size(), 2);
    assert_eq!(v.index_at_offset(0), Some(0));
    assert_eq!(v.index_at_offset(1), Some(0));
    assert_eq!(v.index_at_offset(2), Some(0));

    // Grow: previous last item should start accounting for a trailing gap.
    v.set_count(2);
    // (2 + gap) + 2 = 5
    assert_eq!(v.total_size(), 5);
    assert_eq!(v.index_at_offset(0), Some(0));
    assert_eq!(v.index_at_offset(1), Some(0));
    assert_eq!(v.index_at_offset(2), Some(0)); // inside gap treated as previous
    assert_eq!(v.index_at_offset(3), Some(1));
    assert_eq!(v.index_at_offset(4), Some(1));

    // Shrink: new last item should drop the trailing gap again.
    v.set_count(1);
    assert_eq!(v.total_size(), 2);
    assert_eq!(v.index_at_offset(0), Some(0));
    assert_eq!(v.index_at_offset(1), Some(0));
    assert_eq!(v.index_at_offset(2), Some(0));
}

#[test]
fn set_count_preserves_existing_sizes_and_appends_estimates() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(2, |_| 1));
    v.measure(0, 10);
    assert_eq!(v.total_size(), 11);

    v.set_count(4);
    assert_eq!(v.item_size(0), Some(10));
    assert_eq!(v.item_size(1), Some(1));
    assert_eq!(v.item_size(2), Some(1));
    assert_eq!(v.item_size(3), Some(1));
    assert_eq!(v.total_size(), 13);

    v.set_count(1);
    assert_eq!(v.item_size(0), Some(10));
    assert_eq!(v.item_size(1), None);
    assert_eq!(v.total_size(), 10);
}

#[test]
fn set_count_roundtrips_measured_sizes_across_shrink_and_grow() {
    let mut opts = VirtualizerOptions::new(2, |_| 1);
    opts.gap = 1;
    let mut v = Virtualizer::new(opts);

    v.measure(0, 5);
    v.set_count(4);
    v.measure(3, 7);

    // sizes = [5,1,1,7], total = sum(sizes) + gap*(n-1) = 14 + 3 = 17
    assert_eq!(v.total_size(), 17);
    assert_eq!(v.item_start(3), Some(10));
    assert_eq!(v.item_end(3), Some(17));

    v.set_count(2);
    // sizes = [5,1], total = 6 + gap = 7
    assert_eq!(v.total_size(), 7);
    assert_eq!(v.item_size(0), Some(5));
    assert_eq!(v.item_size(1), Some(1));
    assert_eq!(v.item_size(3), None);

    v.set_count(4);
    assert_eq!(v.item_size(3), Some(7));
    assert_eq!(v.item_start(3), Some(10));
}

#[test]
fn set_count_to_zero_then_grow_is_well_defined() {
    let mut opts = VirtualizerOptions::new(3, |_| 2);
    opts.gap = 1;
    let mut v = Virtualizer::new(opts);
    assert_eq!(v.total_size(), 2 + 1 + 2 + 1 + 2);

    v.set_count(0);
    assert_eq!(v.total_size(), 0);
    assert_eq!(v.index_at_offset(0), None);
    assert!(v.virtual_range().is_empty());

    v.set_count(2);
    assert_eq!(v.total_size(), 2 + 1 + 2);
    assert_eq!(v.index_at_offset(0), Some(0));
    assert_eq!(v.index_at_offset(2), Some(0)); // gap maps to previous
    assert_eq!(v.index_at_offset(3), Some(1));
}

#[test]
fn set_options_count_only_change_preserves_cached_measurements() {
    let mut opts = VirtualizerOptions::new(2, |_| 1);
    opts.gap = 1;
    let mut v = Virtualizer::new(opts);
    v.measure(1, 9);

    let mut next = v.options().clone();
    next.count = 4;
    v.set_options(next);
    assert_eq!(v.item_size(1), Some(9));

    let mut next = v.options().clone();
    next.count = 1;
    v.set_options(next);
    assert_eq!(v.total_size(), 1);
    assert_eq!(v.item_size(0), Some(1));
    assert_eq!(v.item_size(1), None);
}

#[test]
fn scroll_to_index_offset_respects_padding_margin_gap_and_scroll_padding() {
    let mut opts = VirtualizerOptions::new(3, |_| 2);
    opts.gap = 1;
    opts.padding_start = 10;
    opts.scroll_margin = 50;
    opts.scroll_padding_start = 5;
    opts.scroll_padding_end = 4;

    let mut v = Virtualizer::new(opts);
    v.set_viewport_size(10);

    // Starts (including margin):
    // - item0 start = margin(50) + padding_start(10) = 60
    // - item1 start = 60 + (2 + gap=1) = 63
    assert_eq!(v.item_start(0), Some(60));
    assert_eq!(v.item_start(1), Some(63));

    // Align::Start subtracts scroll_padding_start.
    assert_eq!(v.scroll_to_index_offset(1, Align::Start), 58);

    // Align::End uses item end (+scroll_padding_end) - viewport_size.
    // item0 end = 62; 62 + 4 - 10 = 56
    assert_eq!(v.scroll_to_index_offset(0, Align::End), 56);
}

#[test]
fn align_auto_returns_current_offset_when_fully_visible() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(10, |_| 1));
    v.set_viewport_size(5);
    v.set_scroll_offset(3);

    // Viewport covers [3, 8). Item 4 is [4, 5), fully visible.
    assert_eq!(v.scroll_to_index_offset(4, Align::Auto), 3);
}

#[test]
fn align_auto_scrolls_to_end_when_item_is_after_viewport() {
    let mut opts = VirtualizerOptions::new(10, |_| 1);
    opts.scroll_padding_end = 2;
    let mut v = Virtualizer::new(opts);
    v.set_viewport_size(5);
    v.set_scroll_offset(3);

    // Item 9 is after the viewport; Align::Auto should behave like Align::End.
    assert_eq!(
        v.scroll_to_index_offset(9, Align::Auto),
        v.scroll_to_index_offset(9, Align::End)
    );

    // Note: offsets are always clamped to `max_scroll_offset` (no overscroll).
    assert_eq!(
        v.scroll_to_index_offset(9, Align::Auto),
        v.max_scroll_offset()
    );
}

#[test]
fn range_extractor_dedupes_consecutive_duplicates() {
    let mut opts = VirtualizerOptions::new(10, |_| 1);
    opts.overscan = 0;
    opts.range_extractor = Some(Arc::new(|r: Range, emit: &mut dyn FnMut(usize)| {
        // Emit duplicates; contract allows duplicates and requires sorted order.
        emit(r.start_index);
        emit(r.start_index);
        emit(r.start_index + 1);
        emit(r.start_index + 1);
    }));

    let mut v = Virtualizer::new(opts);
    v.set_viewport_size(3);
    v.set_scroll_offset(0);

    let mut out = Vec::new();
    v.for_each_virtual_index(|i| out.push(i));
    assert_eq!(out, vec![0, 1]);
}

#[test]
fn measure_many_marks_items_measured_and_updates_total() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(4, |_| 1));
    assert_eq!(v.total_size(), 4);
    assert!(!v.is_measured(0));
    assert!(!v.is_measured(3));

    v.measure_many([(0, 10), (3, 7)]);
    assert!(v.is_measured(0));
    assert!(v.is_measured(3));
    assert_eq!(v.item_size(0), Some(10));
    assert_eq!(v.item_size(3), Some(7));
    assert_eq!(v.total_size(), 10 + 1 + 1 + 7);
}

#[test]
fn resize_item_many_applies_scroll_offset_adjustment_when_items_are_before_viewport() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(4, |_| 1));
    v.set_viewport_size(2);
    v.set_scroll_offset(100);

    let applied = v.resize_item_many([(0, 4), (2, 0)]);
    // deltas: +3 and -1
    assert_eq!(applied, 2);
    assert_eq!(v.scroll_offset(), 102);
    assert_eq!(v.item_size(0), Some(4));
    assert_eq!(v.item_size(2), Some(0));
}

#[test]
fn sync_item_keys_can_move_measurements_without_replacing_get_item_key() {
    use std::sync::Mutex;

    let keys = Arc::new(Mutex::new(vec![0u64, 1, 2]));
    let mut v = Virtualizer::new(VirtualizerOptions::new_with_key(3, |_| 1, {
        let keys = Arc::clone(&keys);
        move |i| keys.lock().unwrap()[i]
    }));

    v.measure(0, 10);
    assert_eq!(v.item_size(0), Some(10));
    assert_eq!(v.item_size(2), Some(1));

    // Reorder data while keeping the same get_item_key closure (adapter mutates the key mapping).
    *keys.lock().unwrap() = vec![2u64, 1, 0];
    v.sync_item_keys();

    // key=0 measurement should now be at index 2.
    assert_eq!(v.item_size(0), Some(1));
    assert_eq!(v.item_size(2), Some(10));
}

#[test]
fn disabled_virtualizer_is_empty_and_side_effect_free() {
    let mut opts = VirtualizerOptions::new(10, |_| 1);
    opts.enabled = false;
    let mut v = Virtualizer::new(opts);

    assert_eq!(v.total_size(), 0);
    assert!(v.virtual_range().is_empty());
    assert!(v.visible_range().is_empty());
    assert_eq!(v.index_at_offset(0), None);

    // Setters should not panic and should keep returning empty results.
    v.set_viewport_and_scroll_clamped(10, 5);
    assert!(v.virtual_range().is_empty());
}

#[test]
fn scroll_offset_in_list_subtracts_scroll_margin() {
    let mut opts = VirtualizerOptions::new(10, |_| 1);
    opts.scroll_margin = 50;
    let mut v = Virtualizer::new(opts);

    v.set_scroll_offset(0);
    assert_eq!(v.scroll_offset_in_list(), 0);

    v.set_scroll_offset(49);
    assert_eq!(v.scroll_offset_in_list(), 0);

    v.set_scroll_offset(50);
    assert_eq!(v.scroll_offset_in_list(), 0);

    v.set_scroll_offset(55);
    assert_eq!(v.scroll_offset_in_list(), 5);
}

#[test]
fn set_gap_rebuilds_prefix_sums_and_index_mapping() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(2, |_| 2));
    assert_eq!(v.total_size(), 4);
    assert_eq!(v.index_at_offset(2), Some(1));

    v.set_gap(1);
    // Layout: item0(0..2), gap(2..3), item1(3..5)
    assert_eq!(v.total_size(), 5);
    assert_eq!(v.index_at_offset(2), Some(0)); // inside gap treated as previous
    assert_eq!(v.index_at_offset(3), Some(1));
}

#[test]
fn collect_virtual_items_keyed_matches_for_each() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(100, |_| 1));
    v.set_viewport_and_scroll_clamped(10, 50);

    let mut a: Vec<(u64, usize, u64, u32)> = Vec::new();
    v.for_each_virtual_item_keyed(|it| a.push((it.key, it.index, it.start, it.size)));

    let mut b = Vec::new();
    v.collect_virtual_items_keyed(&mut b);
    let b: Vec<(u64, usize, u64, u32)> = b
        .into_iter()
        .map(|it| (it.key, it.index, it.start, it.size))
        .collect();

    assert_eq!(a, b);
}

#[test]
fn collect_virtual_indexes_matches_for_each() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(100, |_| 1));
    v.set_viewport_and_scroll_clamped(10, 50);

    let mut a = Vec::new();
    v.for_each_virtual_index(|i| a.push(i));

    let mut b = Vec::new();
    v.collect_virtual_indexes(&mut b);

    assert_eq!(a, b);
}

#[test]
fn example_basic_smoke_large_count() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(1_000_000, |_| 1));
    v.set_viewport_and_scroll(10, 123_456);

    let r = v.virtual_range();
    assert!(r.start_index <= 123_456);
    assert!(r.end_index >= 123_456);

    let off = v.scroll_to_index_offset(999_999, Align::End);
    assert_eq!(off, 999_990);
    v.set_scroll_offset_clamped(off);
    assert_eq!(v.scroll_offset(), 999_990);
}

#[test]
fn example_dynamic_measurement_smoke() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(100, |_| 10));
    v.set_viewport_and_scroll_clamped(30, 200);

    let applied = v.resize_item(0, 30);
    assert_eq!(applied, 20);
    assert_eq!(v.scroll_offset(), 220);

    let before = v.scroll_offset();
    let applied = v.resize_item(1, 50);
    assert_eq!(applied, 40);
    assert_eq!(v.scroll_offset(), before + 40);

    let before = v.scroll_offset();
    v.measure_unadjusted(2, 50);
    assert_eq!(v.scroll_offset(), before);

    let to = v.scroll_to_index_offset(10, Align::Start);
    v.set_scroll_offset_clamped(to);
    assert_eq!(v.scroll_offset(), 200);

    let mut saw_10 = false;
    v.for_each_virtual_item(|it| {
        if it.index == 10 {
            saw_10 = true;
        }
    });
    assert!(saw_10);
}

#[test]
fn example_pinned_headers_smoke() {
    let pinned: Arc<[usize]> = Arc::from([0usize, 10, 20, 30, 40, 999]);

    let mut opts = VirtualizerOptions::new(1_000, |_| 1);
    opts.overscan = 2;
    opts.range_extractor = Some(Arc::new({
        let pinned = Arc::clone(&pinned);
        move |r: Range, emit: &mut dyn FnMut(usize)| {
            let mut e = IndexEmitter::new(r, emit);
            let overscanned_start = r.start_index.saturating_sub(r.overscan);
            let overscanned_end = r.end_index.saturating_add(r.overscan).min(r.count);

            for &idx in pinned.iter() {
                if idx < overscanned_start {
                    e.emit_pinned(idx);
                }
            }

            e.emit_overscanned();

            for &idx in pinned.iter() {
                if idx >= overscanned_end {
                    e.emit_pinned(idx);
                }
            }
        }
    }));

    let mut v = Virtualizer::new(opts);
    v.set_viewport_and_scroll_clamped(10, 500);

    let mut indexes = Vec::new();
    v.for_each_virtual_index(|i| indexes.push(i));

    // Sorted output (range_extractor contract).
    assert!(indexes.windows(2).all(|w| w[0] < w[1]));

    // Overscanned contiguous range should be present (fixed-size list => deterministic).
    assert!(indexes.contains(&498));
    assert!(indexes.contains(&511));

    // Pinned headers before and after the overscanned range should be present.
    for &idx in pinned.iter() {
        assert!(indexes.contains(&idx));
    }
}

#[test]
fn example_adapter_sim_smoke() {
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicU64, Ordering};

    let saved_scroll = Arc::new(AtomicU64::new(120));

    let opts = VirtualizerOptions::new(1_000, |_| 1)
        .with_initial_rect(Some(Rect {
            main: 10,
            cross: 80,
        }))
        .with_initial_offset_provider({
            let saved_scroll = Arc::clone(&saved_scroll);
            move || saved_scroll.load(Ordering::Relaxed)
        })
        .with_scroll_margin(5)
        .with_range_extractor(Some(|r: Range, emit: &mut dyn FnMut(usize)| {
            let mut e = IndexEmitter::new(r, emit);
            e.emit_pinned(0);
            e.emit_visible();
        }));

    let mut v: Virtualizer<u64> = Virtualizer::new(opts);
    assert_eq!(v.scroll_offset(), 120);
    assert_eq!(
        v.scroll_rect(),
        Rect {
            main: 10,
            cross: 80
        }
    );

    v.apply_scroll_frame(
        Rect {
            main: 12,
            cross: 80,
        },
        200,
        0,
    );
    assert!(v.is_scrolling());

    let mut saw_pinned = false;
    let mut saw_visible = false;
    v.for_each_virtual_item(|it| {
        if it.index == 0 {
            saw_pinned = true;
        }
        if it.index == 200 {
            saw_visible = true;
        }
    });
    assert!(saw_pinned);
    assert!(saw_visible);

    let to = v.scroll_to_index_offset(500, Align::Start);
    v.set_scroll_offset_clamped(to);
    assert_eq!(v.scroll_offset(), to);

    let prev = v.scroll_offset();
    let applied = v.resize_item(0, 20);
    assert_eq!(applied, 19);
    assert_eq!(v.scroll_offset(), prev + 19);

    // Simulate a safe "swap" reorder by changing the key mapping.
    v.set_get_item_key(|i| {
        if i == 0 {
            1
        } else if i == 1 {
            0
        } else {
            i as u64
        }
    });
    assert_eq!(v.item_size(0), Some(1));

    // Debounced scrolling reset.
    v.update_scrolling(200);
    assert!(!v.is_scrolling());

    // Disable all queries.
    v.set_enabled(false);
    assert_eq!(v.total_size(), 0);
    assert!(v.virtual_range().is_empty());
}

#[test]
fn example_tween_scroll_retarget_smoke() {
    #[derive(Clone, Copy, Debug)]
    struct Tween {
        from: u64,
        to: u64,
        start_ms: u64,
        duration_ms: u64,
    }

    impl Tween {
        fn new(from: u64, to: u64, start_ms: u64, duration_ms: u64) -> Self {
            Self {
                from,
                to,
                start_ms,
                duration_ms: duration_ms.max(1),
            }
        }

        fn is_done(&self, now_ms: u64) -> bool {
            now_ms.saturating_sub(self.start_ms) >= self.duration_ms
        }

        fn sample(&self, now_ms: u64) -> u64 {
            let elapsed = now_ms.saturating_sub(self.start_ms);
            let t = (elapsed as f32 / self.duration_ms as f32).clamp(0.0, 1.0);
            let eased = t * t * (3.0 - 2.0 * t); // smoothstep

            let from = self.from as f32;
            let to = self.to as f32;
            (from + (to - from) * eased).max(0.0) as u64
        }

        fn retarget(&mut self, now_ms: u64, new_to: u64, duration_ms: u64) {
            let cur = self.sample(now_ms);
            *self = Self::new(cur, new_to, now_ms, duration_ms);
        }
    }

    let mut v = Virtualizer::new(VirtualizerOptions::new(10_000, |_| 1));
    v.set_viewport_and_scroll_clamped(20, 0);

    let to1 = v.scroll_to_index_offset(2_000, Align::Center);
    let mut tween = Tween::new(v.scroll_offset(), to1, 0, 240);

    let mut now_ms = 0u64;
    let mut last = v.scroll_offset();

    loop {
        now_ms = now_ms.saturating_add(16);
        let off = tween.sample(now_ms);
        v.apply_scroll_offset_event_clamped(off, now_ms);
        assert!(v.scroll_offset() >= last);
        last = v.scroll_offset();

        if (120..120 + 16).contains(&now_ms) {
            let to2 = v.scroll_to_index_offset(7_500, Align::Start);
            tween.retarget(now_ms, to2, 300);
        }

        if tween.is_done(now_ms) {
            break;
        }
    }

    // Finish the animation and mark scrolling as ended.
    v.apply_scroll_offset_event_clamped(
        tween.sample(now_ms + tween.duration_ms),
        now_ms + tween.duration_ms,
    );
    v.set_is_scrolling(false);

    let expected = v.scroll_to_index_offset(7_500, Align::Start);
    assert_eq!(v.scroll_offset(), expected);
    assert!(!v.virtual_range().is_empty());
}

#[test]
fn property_random_layout_invariants() {
    // Fixed seeds => deterministic, non-flaky "property" coverage.
    for seed in [1u64, 2, 3, 4, 5, 123, 999] {
        let mut rng = Lcg::new(seed);

        let count = rng.gen_range_usize(1, 128);
        let gap = rng.gen_range_u32(0, 6);
        let padding_start = rng.gen_range_u32(0, 11);
        let padding_end = rng.gen_range_u32(0, 11);
        let scroll_margin = rng.gen_range_u32(0, 11);
        let overscan = rng.gen_range_usize(0, 5);

        // Start with strictly positive sizes so item starts are strictly increasing, which makes
        // offset->index mapping unambiguous at exact item starts.
        let mut sizes: Vec<u32> = (0..count).map(|_| rng.gen_range_u32(1, 21)).collect();

        // Use estimates matching our generated sizes so the initial state is fully determined.
        let estimates = Arc::new(sizes.clone());
        let mut opts = VirtualizerOptions::new(count, {
            let estimates = Arc::clone(&estimates);
            move |i| estimates[i]
        });
        opts.gap = gap;
        opts.padding_start = padding_start;
        opts.padding_end = padding_end;
        opts.scroll_margin = scroll_margin;
        opts.overscan = overscan;

        let mut v = Virtualizer::new(opts);

        // Basic size invariants.
        assert_eq!(
            v.total_size(),
            expected_total_size(&sizes, gap, padding_start, padding_end)
        );

        // Item starts + offset mapping invariants.
        for i in 0..count {
            let start_in_list = expected_item_start_in_list(&sizes, gap, padding_start, i);
            let start = (scroll_margin as u64).saturating_add(start_in_list);
            assert_eq!(v.item_start(i), Some(start));
            assert_eq!(v.index_at_offset(start), Some(i));

            // Probe inside the item, if any.
            if sizes[i] > 0 {
                let inside = start.saturating_add((sizes[i] as u64).saturating_sub(1));
                assert_eq!(v.index_at_offset(inside), Some(i));
            }

            // Probe the gap region: should map into previous item.
            if gap > 0 && i + 1 < count {
                let gap_start = start.saturating_add(sizes[i] as u64);
                assert_eq!(v.index_at_offset(gap_start), Some(i));
                if gap > 1 {
                    let mid = gap_start.saturating_add((gap as u64) / 2);
                    assert_eq!(v.index_at_offset(mid), Some(i));
                }
            }
        }

        // visible_range_for / virtual_range_for invariants across random scroll/viewport.
        for _ in 0..20 {
            let viewport = rng.gen_range_u32(0, 51);
            let scroll = if rng.gen_bool() {
                u64::MAX
            } else {
                rng.gen_range_u64(0, 5000)
            };

            let expected = expected_visible_range(
                &sizes,
                gap,
                padding_start,
                padding_end,
                scroll_margin,
                scroll,
                viewport,
            );
            assert_eq!(v.visible_range_for(scroll, viewport), expected);

            let mut expected_virtual = expected;
            if !expected_virtual.is_empty() {
                expected_virtual.start_index =
                    expected_virtual.start_index.saturating_sub(overscan);
                expected_virtual.end_index =
                    core::cmp::min(count, expected_virtual.end_index.saturating_add(overscan));
            }
            assert_eq!(v.virtual_range_for(scroll, viewport), expected_virtual);
        }

        // Random measurements should preserve invariants and be roundtrippable via cache.
        for _ in 0..10 {
            let idx = rng.gen_range_usize(0, count);
            let new_size = rng.gen_range_u32(0, 41);
            sizes[idx] = new_size;
            v.measure(idx, new_size);
        }

        assert_eq!(
            v.total_size(),
            expected_total_size(&sizes, gap, padding_start, padding_end)
        );

        // Spot-check offset->index mapping after measurements (may include zero-sized items).
        for _ in 0..50 {
            let off_in_list = rng.gen_range_u64(
                0,
                expected_total_size(&sizes, gap, padding_start, padding_end).saturating_add(20),
            );
            let off = (scroll_margin as u64).saturating_add(off_in_list);
            assert_eq!(
                v.index_at_offset(off),
                expected_index_at_offset_in_list(&sizes, gap, padding_start, off_in_list)
            );
        }

        let snapshot = v.export_measurement_cache();
        let mut v2 = Virtualizer::new(v.options().clone());
        v2.import_measurement_cache(snapshot);

        for i in 0..count {
            assert_eq!(v2.item_size(i), v.item_size(i));
        }
    }
}

#[test]
fn property_keyed_reorder_sync_item_keys_preserves_measurements_by_key() {
    for seed in [42u64, 1337, 2025] {
        let mut rng = Lcg::new(seed);
        let count = rng.gen_range_usize(1, 64);

        // Stable keys are independent of index.
        let mut keys: Vec<u64> = (0..count as u64)
            .map(|_| rng.next_u64() ^ 0x9e3779b97f4a7c15)
            .collect();
        // Ensure uniqueness deterministically (poor man's dedupe).
        keys.sort_unstable();
        keys.dedup();
        while keys.len() < count {
            keys.push(rng.next_u64());
            keys.sort_unstable();
            keys.dedup();
        }
        keys.truncate(count);

        let key_map = Arc::new(std::sync::RwLock::new(keys.clone()));
        let mut v = Virtualizer::new(VirtualizerOptions::new_with_key(count, |_| 1, {
            let key_map = Arc::clone(&key_map);
            move |i| key_map.read().unwrap()[i]
        }));

        // Measure a subset by current index.
        let mut measured: std::collections::HashMap<u64, u32> = std::collections::HashMap::new();
        for _ in 0..(count / 2).max(1) {
            let idx = rng.gen_range_usize(0, count);
            let sz = rng.gen_range_u32(1, 50);
            let key = v.key_for(idx);
            v.measure(idx, sz);
            measured.insert(key, sz);
        }

        // Reorder keys in-place (adapter-style), keep closure stable, then sync.
        key_map.write().unwrap().reverse();
        v.sync_item_keys();

        // Verify measurements follow keys.
        for (key, sz) in measured {
            // Find the key's new index.
            let idx = key_map
                .read()
                .unwrap()
                .iter()
                .position(|&k| k == key)
                .expect("key must exist");
            assert_eq!(v.item_size(idx), Some(sz));
        }
    }
}

#[test]
fn visible_range_clamps_overscrolled_offsets() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(5, |_| 1));
    v.set_viewport_size(2);

    // Offset far beyond content should be treated as clamped to the max offset.
    let visible = v.visible_range_for(u64::MAX, 2);
    assert_eq!(
        visible,
        VirtualRange {
            start_index: 3,
            end_index: 5
        }
    );

    let virtual_r = v.virtual_range_for(u64::MAX, 2);
    // overscan default is 1, so we include one extra item at the start when possible.
    assert_eq!(
        virtual_r,
        VirtualRange {
            start_index: 2,
            end_index: 5
        }
    );
}

#[test]
fn scroll_margin_affects_visibility_and_item_starts() {
    let mut opts = VirtualizerOptions::new(100, |_| 1);
    opts.scroll_margin = 50;
    let mut v = Virtualizer::new(opts);
    v.set_viewport_size(10);

    // Viewport ends before the list starts.
    v.set_scroll_offset(0);
    let mut items = Vec::new();
    v.for_each_virtual_item(|it| items.push(it));
    assert!(items.is_empty());

    // Viewport overlaps the list.
    v.set_scroll_offset(45);
    items.clear();
    v.for_each_virtual_item(|it| items.push(it));
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
fn reset_measurements_clears_measurements() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(3, |_| 1));
    v.measure(1, 10);
    assert!(v.is_measured(1));
    assert_eq!(v.item_size(1), Some(10));

    v.reset_measurements();
    assert!(!v.is_measured(1));
    assert_eq!(v.item_size(1), Some(1));
}

#[test]
fn measurement_cache_can_roundtrip() {
    let mut v1 = Virtualizer::new(VirtualizerOptions::new(10, |_| 1));
    v1.measure(2, 10);
    v1.measure(5, 42);
    assert_eq!(v1.measurement_cache_len(), 2);

    let snapshot = v1.export_measurement_cache();

    let mut v2 = Virtualizer::new(VirtualizerOptions::new(10, |_| 1));
    assert_eq!(v2.measurement_cache_len(), 0);
    assert_eq!(v2.item_size(2), Some(1));
    assert_eq!(v2.item_size(5), Some(1));

    v2.import_measurement_cache(snapshot);
    assert_eq!(v2.measurement_cache_len(), 2);
    assert_eq!(v2.item_size(2), Some(10));
    assert_eq!(v2.item_size(5), Some(42));
}

#[test]
fn set_scroll_offset_clamped_respects_max_scroll_offset() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(10, |_| 1));
    v.set_viewport_size(3);
    let max = v.max_scroll_offset();
    v.set_scroll_offset_clamped(u64::MAX);
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
fn range_extractor_receives_visible_range_and_overscan() {
    let mut opts = VirtualizerOptions::new(100, |_| 1).with_range_extractor(Some(
        |r: Range, emit: &mut dyn FnMut(usize)| {
            assert_eq!(r.overscan, 1);
            let mut e = IndexEmitter::new(r, emit);
            e.emit_pinned(0);
            e.emit_overscanned();
        },
    ));
    opts.overscan = 1;
    let mut v = Virtualizer::new(opts);
    v.set_viewport_size(5);
    v.set_scroll_offset(50);
    let mut idxs = Vec::new();
    v.for_each_virtual_index(|i| idxs.push(i));
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
fn batch_update_coalesces_on_change() {
    let calls: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let mut v = Virtualizer::new(VirtualizerOptions::new(10, |_| 1).with_on_change(Some({
        let calls = Arc::clone(&calls);
        move |_: &Virtualizer<u64>, _: bool| {
            calls.fetch_add(1, Ordering::Relaxed);
        }
    })));

    v.batch_update(|v| {
        v.set_viewport_size(10);
        v.set_scroll_offset(5);
        v.set_scroll_margin(2);
    });

    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn batch_update_is_nestable() {
    let calls: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let mut v = Virtualizer::new(VirtualizerOptions::new(10, |_| 1).with_on_change(Some({
        let calls = Arc::clone(&calls);
        move |_: &Virtualizer<u64>, _: bool| {
            calls.fetch_add(1, Ordering::Relaxed);
        }
    })));

    v.batch_update(|v| {
        v.set_viewport_size(10);
        v.batch_update(|v| {
            v.set_scroll_offset(5);
            v.set_scroll_margin(2);
        });
    });

    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn no_op_setters_do_not_notify() {
    let calls: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let mut v = Virtualizer::new(VirtualizerOptions::new(10, |_| 1).with_on_change(Some({
        let calls = Arc::clone(&calls);
        move |_: &Virtualizer<u64>, _: bool| {
            calls.fetch_add(1, Ordering::Relaxed);
        }
    })));

    v.set_viewport_size(5);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    v.set_viewport_size(5);
    assert_eq!(calls.load(Ordering::Relaxed), 1);

    v.set_scroll_offset(3);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    v.set_scroll_offset(3);
    assert_eq!(calls.load(Ordering::Relaxed), 2);

    // `set_viewport_size` already set scroll_rect.main to 5, cross defaults to 0.
    v.set_scroll_rect(Rect { main: 5, cross: 0 });
    assert_eq!(calls.load(Ordering::Relaxed), 2);

    v.set_scroll_rect(Rect { main: 5, cross: 20 });
    assert_eq!(calls.load(Ordering::Relaxed), 3);
    v.set_scroll_rect(Rect { main: 5, cross: 20 });
    assert_eq!(calls.load(Ordering::Relaxed), 3);

    v.set_viewport_and_scroll_clamped(5, 3);
    assert_eq!(calls.load(Ordering::Relaxed), 3);
}

#[test]
fn apply_scroll_frame_coalesces_on_change() {
    let calls: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let mut v = Virtualizer::new(VirtualizerOptions::new(10, |_| 1).with_on_change(Some({
        let calls = Arc::clone(&calls);
        move |_: &Virtualizer<u64>, _: bool| {
            calls.fetch_add(1, Ordering::Relaxed);
        }
    })));

    v.apply_scroll_frame(
        Rect {
            main: 10,
            cross: 20,
        },
        5,
        0,
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        v.scroll_rect(),
        Rect {
            main: 10,
            cross: 20
        }
    );
    assert_eq!(v.viewport_size(), 10);
    assert_eq!(v.scroll_offset(), 5);
    assert!(v.is_scrolling());
}

#[test]
fn apply_scroll_frame_clamped_clamps_offset() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(10, |_| 1));
    v.apply_scroll_frame_clamped(Rect { main: 3, cross: 0 }, u64::MAX, 0);
    assert_eq!(v.scroll_offset(), v.max_scroll_offset());
}

#[test]
fn set_options_rebuilds_when_closures_change() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(3, |_| 1));
    assert_eq!(v.item_size(0), Some(1));

    // Same count, different closure: should rebuild estimates.
    v.set_options(VirtualizerOptions::new(3, |_| 2));
    assert_eq!(v.item_size(0), Some(2));
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

#[test]
fn frame_state_can_roundtrip() {
    let mut v1 = Virtualizer::new(VirtualizerOptions::new(100, |_| 1));
    v1.apply_scroll_frame_clamped(
        Rect {
            main: 10,
            cross: 20,
        },
        42,
        100,
    );
    v1.set_is_scrolling(false);

    let state = v1.frame_state();

    let mut v2 = Virtualizer::new(VirtualizerOptions::new(100, |_| 1));
    v2.restore_frame_state(state, 200);

    assert_eq!(
        v2.scroll_rect(),
        Rect {
            main: 10,
            cross: 20
        }
    );
    assert_eq!(v2.viewport_size(), 10);
    assert_eq!(v2.scroll_offset(), 42);
    assert!(!v2.is_scrolling());
}

#[test]
fn scroll_to_index_sets_offset_without_scrolling() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(100, |_| 1));
    v.set_viewport_size(10);
    assert!(!v.is_scrolling());

    let expected = v.scroll_to_index_offset(50, Align::Start);
    let applied = v.scroll_to_index(50, Align::Start);
    assert_eq!(applied, expected);
    assert_eq!(v.scroll_offset(), expected);
    assert!(!v.is_scrolling());
}

#[test]
fn collect_virtual_items_matches_for_each() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(100, |_| 1));
    v.set_viewport_size(10);
    v.set_scroll_offset(50);

    let mut a = Vec::new();
    v.collect_virtual_items(&mut a);

    let mut b = Vec::new();
    v.for_each_virtual_item(|it| b.push(it));

    assert_eq!(a, b);
}
