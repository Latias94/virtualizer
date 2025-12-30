# virtualizer (workspace)

[![CI](https://github.com/Latias94/virtualizer/actions/workflows/ci.yml/badge.svg)](https://github.com/Latias94/virtualizer/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/crates/l/virtualizer.svg)](LICENSE-MIT)

A headless virtualization engine inspired by TanStack Virtual.

This repo is a Cargo workspace with two crates:

- `virtualizer/`: core, UI-agnostic virtualization engine (range math, measurements, caching).
- `virtualizer-adapter/`: optional, framework-neutral adapter helpers (anchoring, tweens, controller patterns).

Core design:

- Headless: does not hold any UI objects (TUI/GUI/framework neutral).
- Adapter-driven: scrolling state, time source, and animations live in your adapter layer.
- Allocation-friendly: zero-allocation iteration APIs for per-frame rendering.

## Crates

- `virtualizer/`: core, UI-agnostic virtualization engine (range math, measurements, caching).
- `virtualizer-adapter/`: optional, framework-neutral adapter helpers (anchoring, tweens, controller patterns).

## Installation

This workspace has not been published yet. Use a git dependency for now:

```toml
[dependencies]
virtualizer = { git = "https://github.com/Latias94/virtualizer" }
virtualizer-adapter = { git = "https://github.com/Latias94/virtualizer" }
```

```toml
[dependencies]
virtualizer = "0.1.0"
virtualizer-adapter = "0.1.0"
```

## Usage

### `virtualizer`

```rust
use virtualizer::{Align, Rect, Virtualizer, VirtualizerOptions};

fn main() {
    let mut v = Virtualizer::new(VirtualizerOptions::new(1_000_000, |_| 1));

    // Adapter provides viewport + scroll offset.
    v.apply_scroll_frame_clamped(Rect { main: 20, cross: 0 }, 100, 0);

    // Per-frame render loop (zero-allocation iteration).
    v.for_each_virtual_item(|it| {
        // draw row `it.index` at `it.start` with height `it.size`
        let _ = (it.index, it.start, it.size);
    });

    // Optional: scroll-to helpers.
    let off = v.scroll_to_index_offset(1234, Align::Start);
    v.set_scroll_offset_clamped(off);
}
```

### `virtualizer-adapter` (optional)

```rust
use virtualizer_adapter::{Controller, Easing};

fn main() {
    let mut c = Controller::new(virtualizer::VirtualizerOptions::new(10_000, |_| 1));
    c.virtualizer_mut().set_viewport_size(20);

    // Adapter-driven tween to index.
    c.start_tween_to_index(500, virtualizer::Align::Start, 0, 250, Easing::SmoothStep);

    // In your frame/timer loop:
    let _maybe_new_offset = c.tick(16);
}
```

Quick commands:

- Tests: `cargo nextest run --workspace`
- Build examples: `cargo build --workspace --examples`
- Run examples:
  - `cargo run -p virtualizer --example adapter_sim`
  - `cargo run -p virtualizer --example pinned_headers`
  - `cargo run -p virtualizer --example tween_scroll`
  - `cargo run -p virtualizer-adapter --example anchor_prepend`
  - `cargo run -p virtualizer-adapter --example controller_tween`

Status:

- `0.1.0` is not published yet; the API may change freely. See `CHANGELOG.md`.
