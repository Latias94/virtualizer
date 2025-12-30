use virtualizer_adapter::Controller;

fn main() {
    // Example: preserve visual scroll position across "prepend" (chat/timeline load older messages).
    //
    // The adapter flow is typically:
    // 1) capture an anchor (key + offset_in_viewport) before data changes
    // 2) apply data changes (count/key mapping)
    // 3) apply the anchor to adjust scroll_offset so the same item stays in the same place
    let mut c = Controller::new(virtualizer::VirtualizerOptions::new_with_key(
        100,
        |_| 1,
        |i| 1000u64 + i as u64,
    ));
    c.virtualizer_mut().set_viewport_and_scroll_clamped(10, 50);

    let anchor = c
        .capture_first_visible_anchor()
        .expect("visible range must not be empty");
    println!(
        "before prepend: off={} anchor={anchor:?}",
        c.virtualizer().scroll_offset()
    );

    // Prepend 10 items; old items shift by +10 indexes.
    c.virtualizer_mut().set_count(110);
    c.virtualizer_mut().set_get_item_key(|i| {
        if i < 10 {
            2000u64 + i as u64
        } else {
            1000u64 + (i - 10) as u64
        }
    });
    // Note: `set_count` and `set_get_item_key` rebuild per-index sizes. In real apps, call
    // `sync_item_keys()` when your underlying dataset is reordered while `count` stays the same.

    // Provide a key -> index mapping for the current dataset (owned by your adapter).
    let ok = c.apply_anchor(&anchor, |k| {
        if (1000..1100).contains(k) {
            Some((*k as usize - 1000) + 10)
        } else if (2000..2010).contains(k) {
            Some(*k as usize - 2000)
        } else {
            None
        }
    });

    println!(
        "after prepend: ok={ok} off={}",
        c.virtualizer().scroll_offset()
    );
}
