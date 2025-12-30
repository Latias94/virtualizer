// Example: pinned/sticky rows via range_extractor + IndexEmitter.
use std::sync::Arc;

use virtualizer::{IndexEmitter, Range, Virtualizer, VirtualizerOptions};

fn main() {
    // Example: pinned/sticky "headers" at fixed indexes.
    let mut opts = VirtualizerOptions::new(1_000, |_| 1);
    opts.overscan = 2;

    let pinned: Arc<[usize]> = Arc::from([0usize, 10, 20, 30, 40, 999]);
    opts.range_extractor = Some(Arc::new({
        let pinned = Arc::clone(&pinned);
        move |r: Range, emit: &mut dyn FnMut(usize)| {
            let mut e = IndexEmitter::new(r, emit);
            // IMPORTANT: indexes must be emitted in ascending order.
            //
            // We want pinned rows both before and after the overscanned range. To keep the output
            // sorted, emit:
            // 1) pinned indexes before the overscanned range
            // 2) the overscanned contiguous range
            // 3) pinned indexes after the overscanned range
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

    let mut collected = Vec::new();
    v.for_each_virtual_index(|i| collected.push(i));

    println!("visible_range={:?}", v.visible_range());
    println!("virtual_range={:?}", v.virtual_range());
    println!(
        "indexes_len={} first_20={:?}",
        collected.len(),
        &collected[..20.min(collected.len())]
    );

    // A real UI would typically iterate items:
    let mut headers = 0usize;
    v.for_each_virtual_item(|it| {
        if pinned.binary_search(&it.index).is_ok() {
            headers += 1;
        }
    });
    println!("pinned_headers_in_output={headers}");
}
