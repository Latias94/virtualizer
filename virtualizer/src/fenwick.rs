use alloc::vec::Vec;
use core::cmp;

#[derive(Clone, Debug)]
pub(crate) struct Fenwick {
    tree: Vec<u64>, // 1-indexed
    total: u64,
    max_bit: usize,
}

impl Fenwick {
    pub(crate) fn new(n: usize) -> Self {
        let max_bit = if n == 0 {
            0
        } else {
            highest_power_of_two_leq(n)
        };
        Self {
            tree: alloc::vec![0; n + 1],
            total: 0,
            max_bit,
        }
    }

    pub(crate) fn from_sizes(sizes: &[u32], gap: u32) -> Self {
        let n = sizes.len();
        let mut tree = alloc::vec![0u64; n + 1];
        let mut total = 0u64;
        let max_bit = if n == 0 {
            0
        } else {
            highest_power_of_two_leq(n)
        };
        let gap = gap as u64;
        for i in 1..=n {
            let mut v = sizes[i - 1] as u64;
            if gap > 0 && i < n {
                v = v.saturating_add(gap);
            }
            total = total.saturating_add(v);
            tree[i] = tree[i].saturating_add(v);
            let j = i + lsb(i);
            if j <= n {
                tree[j] = tree[j].saturating_add(tree[i]);
            }
        }
        Self {
            tree,
            total,
            max_bit,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.tree.len().saturating_sub(1)
    }

    pub(crate) fn truncate(&mut self, new_len: usize) {
        let cur = self.len();
        if new_len >= cur {
            return;
        }
        self.total = self.prefix_sum(new_len);
        self.tree.truncate(new_len + 1);
        self.max_bit = if new_len == 0 {
            0
        } else {
            highest_power_of_two_leq(new_len)
        };
    }

    /// Appends a new value to the end of the Fenwick tree.
    ///
    /// `value` is the per-index value (already including any gap/spacing rules from callers).
    ///
    /// This runs in `O(log n)` due to the internal prefix sum queries needed to initialize the
    /// newly appended internal nodes.
    pub(crate) fn push_value(&mut self, value: u64) {
        let new_len = self.len().saturating_add(1);
        self.tree.push(0);
        self.total = self.total.saturating_add(value);

        // Fenwick tree invariant: tree[i] stores the sum of the last lsb(i) values ending at i.
        // When appending, we can derive the correct initial value by using existing prefix sums.
        let l = lsb(new_len);
        let start_exclusive = new_len.saturating_sub(l);
        let before = self
            .prefix_sum(new_len.saturating_sub(1))
            .saturating_sub(self.prefix_sum(start_exclusive));
        self.tree[new_len] = before.saturating_add(value);

        self.max_bit = highest_power_of_two_leq(new_len);
    }

    pub(crate) fn add(&mut self, index: usize, delta: i64) {
        let n = self.len();
        if index >= n {
            return;
        }
        if delta > 0 {
            self.total = self.total.saturating_add(delta as u64);
        } else if delta < 0 {
            self.total = self.total.saturating_sub((-delta) as u64);
        }
        let mut i = index + 1;
        while i <= n {
            let cur = self.tree[i] as i128;
            let next = cur + delta as i128;
            debug_assert!(
                next >= 0,
                "Fenwick underflow (idx={i}, cur={cur}, delta={delta})"
            );
            self.tree[i] = next.clamp(0, u64::MAX as i128) as u64;
            i += lsb(i);
        }
    }

    pub(crate) fn prefix_sum(&self, count: usize) -> u64 {
        let n = self.len();
        let mut i = cmp::min(count, n);
        let mut sum = 0u64;
        while i > 0 {
            sum = sum.saturating_add(self.tree[i]);
            i &= i - 1;
        }
        sum
    }

    pub(crate) fn total(&self) -> u64 {
        self.total
    }

    /// Returns the number of items whose prefix sum is <= `target`.
    ///
    /// This is useful to map an offset to an index:
    /// - `index = lower_bound(offset)` returns the item index at `offset` (clamped).
    pub(crate) fn lower_bound(&self, mut target: u64) -> usize {
        let n = self.len();
        if n == 0 {
            return 0;
        }

        let mut idx = 0usize;
        let mut bit = self.max_bit;
        while bit != 0 {
            let next = idx + bit;
            if next <= n && self.tree[next] <= target {
                target -= self.tree[next];
                idx = next;
            }
            bit >>= 1;
        }
        idx
    }
}

fn lsb(i: usize) -> usize {
    i & i.wrapping_neg()
}

fn highest_power_of_two_leq(n: usize) -> usize {
    let mut p = 1usize;
    while p <= n / 2 {
        p <<= 1;
    }
    p
}
