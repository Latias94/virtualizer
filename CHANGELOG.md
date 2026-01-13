# Changelog

This repository is a Cargo workspace:

- `virtualizer`: core, UI-agnostic virtualization engine.
- `virtualizer-adapter`: optional adapter utilities (anchoring, tweens).

This project follows a pragmatic changelog format during early development. Version numbers follow
SemVer, but the public API may change rapidly until `1.0`.

## [Unreleased]

- TBD

## [0.4.0] - 2026-01-13

### Changed

- API: `measure` / `measure_keyed` / `measure_many` now behave like TanStack Virtual's "measure element":
  they may adjust `scroll_offset` to prevent jumps when measuring items before the viewport.
- API: added `measure_unadjusted` / `measure_keyed_unadjusted` / `measure_many_unadjusted` for callers who
  want measurement updates without scroll adjustment.
- Perf: `resize_item_many` now batches notifications via `batch_update`.
- Ergonomics: `VirtualRange` adds `end_inclusive` / `as_inclusive` helpers.

## [0.3.0] - 2025-12-31

### Changed

- Performance: optimize count-only resizes (avoid full rebuild for append/truncate).

### Tests

- Add example-parity coverage for core and adapter workflows.

## [0.2.0] - 2025-12-30

### Added

- Convenience APIs: `scroll_to_index` and `collect_virtual_*` helpers.

## [0.1.0] - 2025-12-30

Initial release.

### Added

- Optional `serde` feature for serializing public data types.
- Optional `tracing` feature for internal instrumentation.
- Serializable state snapshots (`ViewportState`/`ScrollState`/`FrameState`) and restore helpers.

### virtualizer

- Unify the public API around zero-allocation iteration (`for_each_virtual_*`).
- Unify range extraction to a single `range_extractor` that emits indexes via an `emit` callback.
- Keep scrolling as an adapter concern; core provides `scroll_to_index_offset` and clamping helpers.
- Add measurement cache export/import helpers (`export_measurement_cache`, `import_measurement_cache`).
- Prefix sums + fast offset → index lookup (Fenwick tree).
- Overscanned visible ranges and scroll-to-index helpers.
- Optional dynamic measurement via `measure` / `resize_item`.

### virtualizer-adapter

- Add anchoring helpers for prepend workflows (chat/timelines).
- Add tween helpers and a framework-neutral controller for adapter-driven smooth scrolling.
