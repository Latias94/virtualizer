//! A headless virtualization engine inspired by TanStack Virtual.
//!
//! For adapter-level utilities (anchoring, tweens), see the `virtualizer-adapter` crate.
//!
//! This crate focuses on the core algorithms needed to render massive lists at interactive
//! frame rates: prefix sums over item sizes, fast offset → index lookup, overscanned visible
//! ranges, and optional dynamic measurement.
//!
//! It is UI-agnostic. A TUI/GUI layer is expected to provide:
//! - viewport size (height/width)
//! - scroll offset
//! - item size estimates and (optionally) dynamic measurements
//!
//! # Feature flags
//!
//! - `std` (default): Enables `std` support.
//! - `serde`: Adds `serde::Serialize`/`Deserialize` for the public data types (ranges/items/state).
//! - `tracing`: Emits internal trace/debug/warn events via `tracing` (requires `std`).
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

#[macro_use]
mod macros;

mod emitter;
mod fenwick;
mod key;
mod options;
mod state;
mod types;
mod virtualizer;

#[cfg(test)]
mod tests;

pub use emitter::IndexEmitter;
pub use options::{
    InitialOffset, OnChangeCallback, RangeExtractor,
    ShouldAdjustScrollPositionOnItemSizeChangeCallback, VirtualizerOptions,
};
pub use state::{FrameState, ScrollState, ViewportState};
pub use types::{
    Align, ItemKey, Range, Rect, ScrollDirection, VirtualItem, VirtualItemKeyed, VirtualRange,
};
pub use virtualizer::Virtualizer;

#[doc(hidden)]
pub use key::KeyCacheKey;
