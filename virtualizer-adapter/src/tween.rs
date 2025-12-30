/// A small tween helper for adapter-driven smooth scrolling.
///
/// This is deliberately minimal and framework-neutral:
/// - The adapter provides the time source (`now_ms`).
/// - The adapter applies the returned scroll offset to the real scroll container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Tween {
    pub from: u64,
    pub to: u64,
    pub start_ms: u64,
    pub duration_ms: u64,
    pub easing: Easing,
}

impl Tween {
    /// Creates a tween.
    ///
    /// `duration_ms` is clamped to at least 1ms.
    pub fn new(from: u64, to: u64, start_ms: u64, duration_ms: u64, easing: Easing) -> Self {
        Self {
            from,
            to,
            start_ms,
            duration_ms: duration_ms.max(1),
            easing,
        }
    }

    pub fn is_done(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.start_ms) >= self.duration_ms
    }

    /// Samples the tween at time `now_ms`.
    pub fn sample(&self, now_ms: u64) -> u64 {
        let elapsed = now_ms.saturating_sub(self.start_ms);
        let t = (elapsed as f32 / self.duration_ms as f32).clamp(0.0, 1.0);
        let eased = self.easing.sample(t);

        let from = self.from as f32;
        let to = self.to as f32;
        let v = from + (to - from) * eased;
        v.max(0.0) as u64
    }

    /// Retargets the tween while preserving continuity at `now_ms`.
    pub fn retarget(&mut self, now_ms: u64, new_to: u64, duration_ms: u64) {
        let cur = self.sample(now_ms);
        *self = Self::new(cur, new_to, now_ms, duration_ms, self.easing);
    }
}

/// Built-in easing curves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Easing {
    Linear,
    SmoothStep,
    EaseInOutCubic,
}

impl Easing {
    /// Samples the easing curve with `t` in `[0, 1]`.
    pub fn sample(self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::SmoothStep => t * t * (3.0 - 2.0 * t),
            Self::EaseInOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = -2.0 * t + 2.0;
                    1.0 - (u * u * u) / 2.0
                }
            }
        }
    }
}
