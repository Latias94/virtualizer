# Examples

This crate is UI-agnostic. In practice, you will drive it from an adapter that owns:

- the current `scroll_offset`
- the current viewport size (and optionally a scroll rect)
- any animation/tween state

All examples are runnable with `cargo run -p virtualizer --example <name>`.

## Index

- `basic`: minimal usage, prints the first visible item.
- `adapter_sim`: a "framework adapter" walkthrough (rect updates, scroll events, pinned header,
  dynamic measurement, disabling).
- `tween_scroll`: scroll-to-index animation driven by a simple tween (retarget on interruption).
- `pinned_headers`: pinned/sticky rows via `range_extractor` + `IndexEmitter` (zero allocations in
  the virtualizer).
- `dynamic_measurement`: `measure` / `resize_item` behavior and scroll jump prevention.
- `measurement_cache`: export/import measurement cache for persistence.
- `keyed_reorder`: stable measurement cache across reorders via custom keys + `sync_item_keys`.
  For prepend anchoring (chat/timelines), see `cargo run -p virtualizer-adapter --example anchor_prepend`.

## Typical Adapter Loop

Most integrations look like this:

```rust
// 1) Adapter updates viewport + scroll offset (from events / animations)
v.set_viewport_and_scroll_clamped(viewport_main, scroll_offset);

// 2) Optional: maintain scrolling state (TanStack-style)
v.notify_scroll_event(now_ms);
// ... later ...
v.update_scrolling(now_ms);

// 3) Render: iterate the virtual items for this frame
v.for_each_virtual_item(|it| {
    // draw item at it.start with it.size
});
```

If your adapter updates multiple fields in response to a single event/frame, consider using
`batch_update` to coalesce `on_change` notifications:

```rust
v.batch_update(|v| {
    v.set_scroll_rect(rect);
    v.set_scroll_offset_clamped(scroll_offset);
    v.notify_scroll_event(now_ms);
});
```

Alternatively, use the dedicated event-style entry points:

```rust
v.apply_scroll_offset_event_clamped(scroll_offset, now_ms);
// or, if you also have rect information:
v.apply_scroll_frame_clamped(rect, scroll_offset, now_ms);
```

Key idea: the adapter owns the time source and the scroll state; the virtualizer provides stable,
fast range computation and layout math.

## Scenario Notes & Pitfalls

### Tween / Smooth Scrolling (`tween_scroll`)

How it works:

- Compute the target offset with `scroll_to_index_offset`.
- Sample your tween every frame.
- Feed it into `set_scroll_offset_clamped`.

```rust
let target = v.scroll_to_index_offset(index, Align::Center);
// each frame:
v.set_scroll_offset_clamped(tween.sample(now_ms));
v.for_each_virtual_item(|it| draw(it));
```

Pitfalls:

- **Interruptions:** user wheel/drag should retarget or cancel the tween (sample current offset,
  then retarget `to`).
- **Clamping:** always clamp offsets (either via `*_clamped` APIs or by calling `clamp_scroll_offset`)
  so animations don’t drift beyond content bounds.
- **Scrolling state:** if your UI uses `is_scrolling`, keep it consistent with animation frames.

### Pinned / Sticky Rows (`pinned_headers`)

Use `range_extractor` to customize the exact set of indexes rendered each frame (pinned headers,
group rows, sentinels, etc.). The extractor is *zero-allocation*: it emits indexes via a callback.

The extractor receives a `Range` describing:

- the **visible** range (`start_index..end_index`, no overscan)
- `overscan` to help you compute overscanned ranges
- `count` (upper bound)

Prefer `IndexEmitter` for correctness and ergonomics:

```rust
opts.range_extractor = Some(Arc::new(|r: Range, emit: &mut dyn FnMut(usize)| {
    let mut e = IndexEmitter::new(r, emit);
    e.emit_pinned(0);
    e.emit_overscanned();
}));
```

Pitfalls:

- **Order matters:** indexes should be emitted in ascending order; duplicates are ignored.
- **Performance:** when you use an extractor, the virtualizer cannot use the contiguous fast path
  and will iterate by index.

### Dynamic Measurement (`dynamic_measurement`)

- `measure(index, size)` updates sizes but does **not** adjust scroll position.
- `resize_item(index, size)` returns an applied scroll delta and also updates internal
  `scroll_offset` to prevent jumps when items *before* the viewport change size.

Rule of thumb:

- use `measure` when you want “just update the cache”
- use `resize_item` when resizing might cause a visible jump and you want to compensate

### Keyed Reorder (`keyed_reorder`)

If you keep `count` the same but reorder your underlying data, measured sizes should follow the
*item identity*, not the index. Use custom keys and call `sync_item_keys` after reorders (when
`count` stays the same):

```rust
v.sync_item_keys();
```

Pitfalls:

- If your dataset changes but you forget to call `sync_item_keys`, cached measurements may “stick”
  to the wrong indexes.
- If you do change `get_item_key` via `set_get_item_key`, the virtualizer automatically rebuilds
  per-index sizes from the key-based cache.

### Prepend Anchoring (workspace `virtualizer-adapter`)

If you load more data *above* the current viewport (chat/timeline prepend), you typically want to
preserve the user's view (avoid jumping). Use an anchor:

1) capture an anchor before data changes (`key + offset_in_viewport`)
2) update data (`count` + key mapping)
3) apply the anchor by setting a new scroll offset

The workspace `virtualizer-adapter` crate provides helpers:

- `capture_first_visible_anchor`
- `apply_anchor`

Runnable example:

- `cargo run -p virtualizer-adapter --example anchor_prepend`
- `cargo run -p virtualizer-adapter --example controller_tween`
