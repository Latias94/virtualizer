# virtualizer

[![CI](https://github.com/Latias94/virtualizer/actions/workflows/ci.yml/badge.svg)](https://github.com/Latias94/virtualizer/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/virtualizer.svg)](https://crates.io/crates/virtualizer)
[![docs.rs](https://docs.rs/virtualizer/badge.svg)](https://docs.rs/virtualizer)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/crates/l/virtualizer.svg)](LICENSE-MIT)

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

let mut items = Vec::new();
v.for_each_virtual_item(|it| items.push(it));
assert!(!items.is_empty());

let off = v.scroll_to_index_offset(1234, Align::Start);
v.set_scroll_offset(off);
```

If you want clamped scroll offsets (useful for scroll-to helpers), use `set_scroll_offset_clamped`
or `clamp_scroll_offset`.

If your UI layer wants keys alongside items, use `for_each_virtual_item_keyed`.

If you want to scroll to a raw offset (TanStack `scrollToOffset`), set it via
`set_scroll_offset_clamped` (or `set_scroll_offset` if you want to handle clamping yourself).

For an adapter-style walkthrough (rect/scroll events/dynamic measurement), see
`examples/adapter_sim.rs` (`cargo run -p virtualizer --example adapter_sim`).

For more end-to-end adapter scenarios (tween scrolling, pinned headers, keyed reorders, dynamic
measurement), see `examples/README.md`.

If you want adapter-level utilities (scroll anchoring for prepend, tween helpers), see the optional
`virtualizer-adapter` crate in this workspace (`../virtualizer-adapter/`).

### Reusing buffers (allocation-friendly)

If you call `for_each_virtual_item` every frame, consider reusing a `Vec` to avoid per-frame
allocations:

```rust
use virtualizer::{Virtualizer, VirtualizerOptions};

let v = Virtualizer::new(VirtualizerOptions::new(10_000, |_| 1));
let mut items = Vec::new();
items.clear();
v.for_each_virtual_item(|it| items.push(it));
```

Similar APIs exist for keys and indexes: `for_each_virtual_item_keyed` and `for_each_virtual_index`.

### Pinned/sticky rows (custom index selection)

If you need pinned/sticky rows (e.g. headers), set `range_extractor` to emit the exact set of
indexes you want to render. The extractor is zero-allocation: it receives an `emit` callback.

Use `IndexEmitter` to make this ergonomic and correct (sorted + deduped):

```rust
use std::sync::Arc;
use virtualizer::{IndexEmitter, Range, VirtualizerOptions};

let mut opts = VirtualizerOptions::new(1000, |_| 1);
opts.range_extractor = Some(Arc::new(|r: Range, emit: &mut dyn FnMut(usize)| {
    let mut e = IndexEmitter::new(r, emit);
    e.emit_pinned(0);
    e.emit_overscanned();
}));
```

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
sizes follow item keys. If you update `get_item_key` itself via `set_get_item_key`, the virtualizer
automatically rebuilds per-index sizes from the key-based cache.

```rust
use virtualizer::{Virtualizer, VirtualizerOptions};

let mut v = Virtualizer::new(VirtualizerOptions::new(2, |_| 1));
v.measure(0, 10);
v.set_get_item_key(|i| if i == 0 { 1 } else { 0 }); // reorder
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

To reset all measurements (TanStack `measure()`), call `reset_measurements`.

To persist and restore measurements (key → size), use `export_measurement_cache` /
`import_measurement_cache` (see `examples/measurement_cache.rs`).

### State & callbacks

If you want TanStack-style change notifications, set `on_change` (or call `set_on_change`) and drive
`is_scrolling` via `set_is_scrolling` from your framework adapter.

For TanStack parity, you can also use `notify_scroll_event(now_ms)` + `update_scrolling(now_ms)` to
implement `isScrollingResetDelay` behavior without requiring timers in the core.

If your adapter updates multiple pieces of state in a single frame (common in UI event handlers),
use `batch_update` to coalesce them into a single `on_change` callback:

```rust
v.batch_update(|v| {
    v.set_scroll_rect(rect);
    v.set_scroll_offset_clamped(offset);
    v.notify_scroll_event(now_ms);
});
```

For even more ergonomic adapter code, you can use the higher-level "event" entry points which
internally batch and mark scrolling:

- `apply_scroll_offset_event` / `apply_scroll_offset_event_clamped`
- `apply_scroll_frame` / `apply_scroll_frame_clamped`

### no_std + alloc

Disable default features to build in `no_std` environments (requires `alloc`):

```sh
cargo build --no-default-features
```

## License

Dual-licensed under `MIT OR Apache-2.0`. See `LICENSE-MIT` and `LICENSE-APACHE`.
