use crate::*;

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::{AtomicU64, Ordering};

static INITIAL_OFFSET_PROVIDER_CALLED: AtomicU64 = AtomicU64::new(0);

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
