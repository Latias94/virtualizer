use virtualizer::{Align, Virtualizer, VirtualizerOptions};

fn main() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(1_000_000, |_| 1));
    v.set_viewport_size(10);
    v.set_scroll_offset(123_456);

    let items = v.get_virtual_items();
    println!("total_size={}", v.get_total_size());
    println!("visible_range={:?}", v.get_virtual_range());
    println!("first_visible={:?}", items.first());

    v.scroll_to_index(999_999, Align::End);
    println!("after scroll_to_index: offset={}", v.scroll_offset());
}
