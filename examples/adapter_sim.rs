use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use virtualizer::{Align, Range, Rect, Virtualizer, VirtualizerOptions};

fn main() {
    // Simulate a framework adapter that owns the scroll state.
    let saved_scroll = Arc::new(AtomicU64::new(120));

    let opts = VirtualizerOptions::new(1000, |_| 1)
        .with_initial_rect(Some(Rect {
            main: 10,
            cross: 80,
        }))
        .with_initial_offset_provider({
            let saved_scroll = Arc::clone(&saved_scroll);
            move || saved_scroll.load(Ordering::Relaxed)
        })
        .with_scroll_margin(5)
        .with_range_extractor_v2(Some(|r: Range| {
            // Pin a "sticky header" at index 0, regardless of scroll position.
            let mut out: Vec<usize> = (r.start_index..r.end_index).collect();
            out.push(0);
            out.sort_unstable();
            out.dedup();
            out
        }));

    let mut v = Virtualizer::new(opts);

    // First render: scroll offset comes from the provider.
    println!("initial scroll_offset={}", v.scroll_offset());
    println!("initial scroll_rect={:?}", v.scroll_rect());

    // Adapter updates rect + scroll offset on events.
    v.set_scroll_rect(Rect {
        main: 12,
        cross: 80,
    });
    v.set_scroll_offset(200);
    v.notify_scroll_event(0);

    let items = v.get_virtual_items_keyed();
    println!(
        "is_scrolling={}, visible_range={:?}, items_len={}",
        v.is_scrolling(),
        v.get_visible_range(),
        items.len()
    );
    println!("first_item={:?}", items.first());

    // Demonstrate scroll-to helpers.
    let target = v.scroll_to_index_offset(500, Align::Start);
    v.scroll_to_offset_clamped(target);
    println!("after scroll_to_index: scroll_offset={}", v.scroll_offset());

    // Demonstrate dynamic measurement + scroll adjustment.
    let applied = v.resize_item(0, 20);
    println!("resize_item applied_scroll_adjustment={applied}");

    // Simulate reorder: change key mapping and sync measurements.
    v.set_get_item_key(|i| if i == 0 { 1 } else { i as u64 });
    v.sync_item_keys();

    // Debounced scrolling reset without relying on a native scrollend event.
    v.update_scrolling(200);
    println!("after update_scrolling: is_scrolling={}", v.is_scrolling());

    // Toggle enabled to disable all queries.
    v.set_enabled(false);
    println!(
        "disabled total_size={}, items_len={}",
        v.get_total_size(),
        v.get_virtual_items().len()
    );
}
