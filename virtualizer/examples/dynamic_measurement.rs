// Example: dynamic measurement and scroll jump prevention.
use virtualizer::{Align, Virtualizer, VirtualizerOptions};

fn main() {
    // Example: dynamic measurement + scroll jump prevention.
    let mut v = Virtualizer::new(VirtualizerOptions::new(100, |_| 10));
    v.set_viewport_and_scroll_clamped(30, 200);

    println!(
        "before: off={} total={} range={:?}",
        v.scroll_offset(),
        v.total_size(),
        v.virtual_range()
    );

    // If an item before the viewport changes size, `resize_item` can adjust scroll_offset to
    // prevent visual jumps.
    let applied = v.resize_item(0, 30);
    println!(
        "resize_item(0): applied_delta={applied} off={} total={}",
        v.scroll_offset(),
        v.total_size()
    );

    // Measuring an item updates sizes and may adjust scroll (TanStack-aligned behavior).
    v.measure(1, 50);
    println!(
        "measure(1): off={} total={}",
        v.scroll_offset(),
        v.total_size()
    );

    // If you want to update measurements without changing scroll, use `measure_unadjusted`.
    v.measure_unadjusted(2, 30);

    // Scroll-to helpers still work with updated measurements.
    let to = v.scroll_to_index_offset(10, Align::Start);
    v.set_scroll_offset_clamped(to);
    println!(
        "set_scroll_offset_clamped(scroll_to_index_offset(10)): off={} range={:?}",
        v.scroll_offset(),
        v.virtual_range()
    );
}
