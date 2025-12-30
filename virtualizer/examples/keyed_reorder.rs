// Example: measurements follow keys after reorder.
use virtualizer::{Virtualizer, VirtualizerOptions};

fn main() {
    // Example: measurements follow keys after reorder.
    let mut v = Virtualizer::new(VirtualizerOptions::new(2, |_| 1));
    v.measure(0, 10);
    println!(
        "before reorder: size0={:?} size1={:?}",
        v.item_size(0),
        v.item_size(1)
    );

    // Simulate data reorder by changing the key mapping.
    v.set_get_item_key(|i| if i == 0 { 1 } else { 0 });
    // Note: in real apps you usually keep `get_item_key` stable and call `sync_item_keys()` when
    // your underlying dataset is reordered while `count` stays the same.

    println!(
        "after reorder: size0={:?} size1={:?}",
        v.item_size(0),
        v.item_size(1)
    );
}
