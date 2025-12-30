# virtualizer

A headless virtualization engine inspired by TanStack Virtual.

This crate provides the core algorithms needed to virtualize massive lists at interactive frame
rates:

- Prefix sums over item sizes (Fenwick tree).
- Fast offset → index lookup.
- Overscanned visible ranges.
- Optional dynamic measurement via `measure` / `resize_item`.
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

If you want clamped scroll offsets (useful for scroll-to helpers), use `set_scroll_offset_clamped`
or `clamp_scroll_offset`.

If your UI layer wants keys alongside items, use `get_virtual_items_keyed`.

If you want to scroll to a raw offset (TanStack `scrollToOffset`), use `scroll_to_offset` /
`scroll_to_offset_clamped`.

For an adapter-style walkthrough (rect/scroll events/dynamic measurement), see
`examples/adapter_sim.rs` (`cargo run --example adapter_sim`).

### Custom keys

Use `VirtualizerOptions::new_with_key` if you want a non-`u64` key type (e.g. for stable caching
across reorders in your UI layer).

```rust
use virtualizer::{Virtualizer, VirtualizerOptions};

let opts = VirtualizerOptions::new_with_key(100, |_| 1, |i| format!("row-{i}"));
let v: Virtualizer<String> = Virtualizer::new(opts);
assert_eq!(v.key_for(7), "row-7");
```

Note: the key type must implement `Hash + Eq` with the default `std` feature, and `Ord` when built
with `--no-default-features` (no_std + alloc).

If your data is reordered/changed while `count` stays the same, call `sync_item_keys` so measured
sizes follow item keys:

```rust
use virtualizer::{Virtualizer, VirtualizerOptions};

let mut v = Virtualizer::new(VirtualizerOptions::new(2, |_| 1));
v.measure(0, 10);
v.set_get_item_key(|i| if i == 0 { 1 } else { 0 }); // reorder
v.sync_item_keys();
assert_eq!(v.item_size(1), Some(10));
```

### Scroll margin

If your scroll offset is measured from a larger scroll container (e.g. window scrolling) and the
list starts after some header/content, set `scroll_margin` so virtual item `start` values match the
scroll container coordinate system:

```rust
use virtualizer::{Virtualizer, VirtualizerOptions};

let mut opts = VirtualizerOptions::new(10_000, |_| 35);
opts.scroll_margin = 50; // list begins 50px below scroll origin
let mut v = Virtualizer::new(opts);
v.set_viewport_and_scroll(500, 0);
```

### Rects

TanStack Virtual exposes `scrollRect` and an `initialRect` option. This crate provides a platform-
agnostic `Rect { main, cross }`:

- `main`: the virtualized axis size (e.g. height for vertical lists)
- `cross`: the cross axis size (e.g. width for vertical lists)

Use `VirtualizerOptions::with_initial_rect(Some(rect))` for an initial value (SSR-like setups), and
call `set_scroll_rect(rect)` from your adapter to keep it up-to-date.

### Initial offset

TanStack Virtual allows `initialOffset` to be a number or a function. This crate uses
`InitialOffset`:

- `InitialOffset::Value(123)`
- `InitialOffset::Provider(|| 123)`

### Dynamic measurement

`measure` updates sizes but does not adjust scroll position. `resize_item` returns the applied scroll
adjustment (delta) and also updates the virtualizer's internal `scroll_offset` to prevent jumps when
items before the viewport change size.

To reset all measurements (TanStack `measure()`), call `measure_all` (or `reset_measurements`).

### State & callbacks

If you want TanStack-style change notifications, set `on_change` (or call `set_on_change`) and drive
`is_scrolling` via `set_is_scrolling` from your framework adapter.

For TanStack parity, you can also use `notify_scroll_event(now_ms)` + `update_scrolling(now_ms)` to
implement `isScrollingResetDelay` behavior without requiring timers in the core.

### no_std + alloc

Disable default features to build in `no_std` environments (requires `alloc`):

```sh
cargo build --no-default-features
```

## License

Dual-licensed under `MIT OR Apache-2.0`. See `LICENSE-MIT` and `LICENSE-APACHE`.
