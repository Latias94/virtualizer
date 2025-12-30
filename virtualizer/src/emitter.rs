use crate::Range;

/// Helper to build correct `range_extractor` implementations without allocations.
///
/// It enforces the extractor contract:
/// - Out-of-bounds indexes are ignored (and debug-asserted).
/// - Duplicates are ignored.
/// - Out-of-order indexes are ignored (and debug-asserted).
pub struct IndexEmitter<'a> {
    range: Range,
    last: Option<usize>,
    emit: &'a mut dyn FnMut(usize),
}

impl<'a> IndexEmitter<'a> {
    pub fn new(range: Range, emit: &'a mut dyn FnMut(usize)) -> Self {
        Self {
            range,
            last: None,
            emit,
        }
    }

    pub fn range(&self) -> Range {
        self.range
    }

    pub fn emit(&mut self, index: usize) {
        if index >= self.range.count {
            vwarn!(
                index,
                count = self.range.count,
                "IndexEmitter: out-of-bounds index"
            );
            debug_assert!(
                index < self.range.count,
                "IndexEmitter: out-of-bounds index (i={index}, count={})",
                self.range.count
            );
            return;
        }

        if let Some(prev) = self.last {
            if index == prev {
                return;
            }
            if index < prev {
                vwarn!(
                    prev,
                    next = index,
                    "IndexEmitter: indexes must be emitted in ascending order"
                );
                debug_assert!(
                    index > prev,
                    "IndexEmitter: indexes must be emitted in ascending order (prev={prev}, next={index})"
                );
                return;
            }
        }

        self.last = Some(index);
        (self.emit)(index);
    }

    pub fn emit_pinned(&mut self, index: usize) {
        self.emit(index);
    }

    pub fn emit_range(&mut self, start_index: usize, end_index: usize) {
        let end = end_index.min(self.range.count);
        for i in start_index..end {
            self.emit(i);
        }
    }

    pub fn emit_visible(&mut self) {
        self.emit_range(self.range.start_index, self.range.end_index);
    }

    pub fn emit_overscanned(&mut self) {
        let start = self.range.start_index.saturating_sub(self.range.overscan);
        let end = self
            .range
            .end_index
            .saturating_add(self.range.overscan)
            .min(self.range.count);
        self.emit_range(start, end);
    }
}
