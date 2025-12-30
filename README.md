# virtualizer (workspace)

[![CI](https://github.com/Latias94/virtualizer/actions/workflows/ci.yml/badge.svg)](https://github.com/Latias94/virtualizer/actions/workflows/ci.yml)
[![crates.io: virtualizer](https://img.shields.io/crates/v/virtualizer.svg)](https://crates.io/crates/virtualizer)
[![docs.rs: virtualizer](https://docs.rs/virtualizer/badge.svg)](https://docs.rs/virtualizer)
[![crates.io: virtualizer-adapter](https://img.shields.io/crates/v/virtualizer-adapter.svg)](https://crates.io/crates/virtualizer-adapter)
[![docs.rs: virtualizer-adapter](https://docs.rs/virtualizer-adapter/badge.svg)](https://docs.rs/virtualizer-adapter)
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

What you get (high level):

- Fast offset → index lookup (Fenwick/prefix sums) + overscanned visible ranges.
- Zero-allocation iteration: `for_each_virtual_index`, `for_each_virtual_item`, `for_each_virtual_item_keyed`.
- Dynamic measurement: `measure` / `resize_item` (with optional scroll-jump prevention).
- Pinned/sticky rows via `range_extractor` + `IndexEmitter` (emit indexes via callback, no `Vec` allocation).
- Measurement cache persistence: `export_measurement_cache` / `import_measurement_cache`.
- Adapter utilities: prepend anchoring + tween-driven smooth scrolling (optional).

Quick commands:

- Tests: `cargo nextest run --workspace`
- Build examples: `cargo build --workspace --examples`
- Run examples:
  - `cargo run -p virtualizer --example adapter_sim`
  - `cargo run -p virtualizer --example pinned_headers`
  - `cargo run -p virtualizer --example tween_scroll`
  - `cargo run -p virtualizer-adapter --example anchor_prepend`
  - `cargo run -p virtualizer-adapter --example controller_tween`

Docs:

- Core crate: `virtualizer/README.md`
- Example index + scenario notes: `virtualizer/examples/README.md`

Status:

- `0.1.0` is not published yet; the API may change freely. See `CHANGELOG.md`.
