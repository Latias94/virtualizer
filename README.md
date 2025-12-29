# virtualizer

A headless virtualization engine inspired by TanStack Virtual.

This crate provides the core algorithms needed to virtualize massive lists at interactive frame
rates:

- Prefix sums over item sizes (Fenwick tree).
- Fast offset → index lookup.
- Overscanned visible ranges.
- Optional dynamic measurement via `measure`.
- Scroll-to-index helpers (`scroll_to_index_offset`, `Align`).

It is UI-agnostic and can be used in TUIs/GUI frameworks.

## Usage

```rust
use virtualizer::{Align, Virtualizer, VirtualizerOptions};

let mut v = Virtualizer::new(VirtualizerOptions::new(1_000_000, |_| 1));
v.set_viewport_size(20);
v.set_scroll_offset(100);

let items = v.get_virtual_items();
assert!(!items.is_empty());

let off = v.scroll_to_index_offset(1234, Align::Start);
v.set_scroll_offset(off);
```

## License

Dual-licensed under `MIT OR Apache-2.0`. See `LICENSE-MIT` and `LICENSE-APACHE`.
