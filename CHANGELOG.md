# Changelog

This repository is a Cargo workspace:

- `virtualizer`: core, UI-agnostic virtualization engine.
- `virtualizer-adapter`: optional adapter utilities (anchoring, tweens).

This project follows a pragmatic changelog format during early development. Version numbers follow
SemVer, but the public API may change rapidly until `1.0`.

## [Unreleased]

- TBD

## [0.1.0] - 2025-12-30

`0.1.0` has not been published yet. The API may change freely until the first release.

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
