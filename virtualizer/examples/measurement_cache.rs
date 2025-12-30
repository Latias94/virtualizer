// Example: export and import measurement cache.
use virtualizer::{Virtualizer, VirtualizerOptions};

fn main() {
    // Example: export and import measurement cache (key -> measured size).
    //
    // This is useful if you want to persist measurements across screens/sessions so that the
    // virtualizer can start with better estimates and avoid re-measuring everything.
    let mut v1 = Virtualizer::new(VirtualizerOptions::new(10, |_| 1));
    v1.measure(2, 10);
    v1.measure(5, 42);

    let snapshot = v1.export_measurement_cache();
    println!("exported_cache_len={}", snapshot.len());

    let mut v2 = Virtualizer::new(VirtualizerOptions::new(10, |_| 1));
    println!(
        "before import: size2={:?} size5={:?}",
        v2.item_size(2),
        v2.item_size(5)
    );

    v2.import_measurement_cache(snapshot);
    println!(
        "after import: cache_len={} size2={:?} size5={:?}",
        v2.measurement_cache_len(),
        v2.item_size(2),
        v2.item_size(5)
    );
}
