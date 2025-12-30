// Example: minimal usage and scroll-to helper.
use virtualizer::{Align, Virtualizer, VirtualizerOptions};

fn main() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(1_000_000, |_| 1));
    v.set_viewport_and_scroll(10, 123_456);

    let mut items = Vec::new();
    v.for_each_virtual_item(|it| items.push(it));
    println!("total_size={}", v.total_size());
    println!("visible_range={:?}", v.virtual_range());
    println!("first_visible={:?}", items.first());

    let off = v.scroll_to_index_offset(999_999, Align::End);
    v.set_scroll_offset_clamped(off);
    println!("after scroll_to_index: offset={}", v.scroll_offset());
}
