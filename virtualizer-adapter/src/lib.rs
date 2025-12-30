//! Adapter utilities for the `virtualizer` crate.
//!
//! The `virtualizer` crate is UI-agnostic and focuses on the core math and state. This crate
//! provides small, framework-neutral helpers commonly needed by adapters:
//!
//! - Scroll anchoring (e.g. prepend in chat/timelines without visual jumps)
//! - Tween-based smooth scrolling helpers (optional; adapter-driven)
//!
//! This crate is intentionally framework-agnostic (no ratatui/egui bindings).
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

mod anchor;
mod controller;
mod key;
mod tween;

#[cfg(test)]
mod tests;

pub use anchor::{ScrollAnchor, apply_anchor, capture_first_visible_anchor};
pub use controller::Controller;
pub use key::VirtualizerKey;
pub use tween::{Easing, Tween};
