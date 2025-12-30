# virtualizer-adapter

[![CI](https://github.com/Latias94/virtualizer/actions/workflows/ci.yml/badge.svg)](https://github.com/Latias94/virtualizer/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/virtualizer-adapter.svg)](https://crates.io/crates/virtualizer-adapter)
[![docs.rs](https://docs.rs/virtualizer-adapter/badge.svg)](https://docs.rs/virtualizer-adapter)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/crates/l/virtualizer-adapter.svg)](LICENSE-MIT)

Adapter utilities for the `virtualizer` crate (workspace sibling).

This crate is intentionally framework-neutral and does not hold any UI objects. It provides small,
common building blocks for adapters:

- Scroll anchoring for prepend workflows (chat/timelines) without visual jumps
- Tween helpers and a simple controller pattern for smooth scrolling (adapter-driven)

This crate is part of the `virtualizer` workspace repository.

## Usage

```rust
use virtualizer_adapter::{Controller, Easing};

let mut c = Controller::new(virtualizer::VirtualizerOptions::new(10_000, |_| 1));
c.virtualizer_mut().set_viewport_size(20);

// Start an adapter-driven tween to an index.
c.start_tween_to_index(500, virtualizer::Align::Start, 0, 250, Easing::SmoothStep);

// In your frame loop:
let _ = c.tick(16);
```

## Examples

- Prepend anchoring: `cargo run -p virtualizer-adapter --example anchor_prepend`
- Tween controller: `cargo run -p virtualizer-adapter --example controller_tween`
