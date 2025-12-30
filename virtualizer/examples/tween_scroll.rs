// Example: adapter-driven tween scrolling (retarget on interruption).
use virtualizer::{Align, Virtualizer, VirtualizerOptions};

#[derive(Clone, Copy, Debug)]
struct Tween {
    from: u64,
    to: u64,
    start_ms: u64,
    duration_ms: u64,
}

impl Tween {
    fn new(from: u64, to: u64, start_ms: u64, duration_ms: u64) -> Self {
        Self {
            from,
            to,
            start_ms,
            duration_ms: duration_ms.max(1),
        }
    }

    fn is_done(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.start_ms) >= self.duration_ms
    }

    fn sample(&self, now_ms: u64) -> u64 {
        let elapsed = now_ms.saturating_sub(self.start_ms);
        let t = (elapsed as f32 / self.duration_ms as f32).clamp(0.0, 1.0);
        let eased = smoothstep(t);

        let from = self.from as f32;
        let to = self.to as f32;
        let v = from + (to - from) * eased;
        v.max(0.0) as u64
    }

    fn retarget(&mut self, now_ms: u64, new_to: u64, duration_ms: u64) {
        let cur = self.sample(now_ms);
        *self = Self::new(cur, new_to, now_ms, duration_ms);
    }
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn main() {
    // Simulate an adapter that drives scroll_offset with a tween.
    let mut v = Virtualizer::new(VirtualizerOptions::new(10_000, |_| 1));
    v.set_viewport_and_scroll_clamped(20, 0);

    let to = v.scroll_to_index_offset(2_000, Align::Center);
    let mut tween = Tween::new(v.scroll_offset(), to, 0, 240);

    let mut now_ms = 0u64;
    let mut frame = 0u64;

    loop {
        // Simulate a 60fps "tick".
        now_ms = now_ms.saturating_add(16);
        frame += 1;

        // Adapter samples tween and writes it into the virtualizer.
        let off = tween.sample(now_ms);
        v.apply_scroll_offset_event_clamped(off, now_ms);

        // Render: a UI would iterate items and draw them.
        let mut first: Option<usize> = None;
        let mut last: Option<usize> = None;
        v.for_each_virtual_item(|it| {
            first.get_or_insert(it.index);
            last = Some(it.index);
        });

        if frame % 5 == 0 {
            println!(
                "t={now_ms}ms off={} visible={:?} first={:?} last={:?}",
                v.scroll_offset(),
                v.visible_range(),
                first,
                last
            );
        }

        // Simulate user input interrupting the animation:
        // at ~120ms, retarget to a new index.
        if (120..120 + 16).contains(&now_ms) {
            let new_to = v.scroll_to_index_offset(7_500, Align::Start);
            tween.retarget(now_ms, new_to, 300);
        }

        if tween.is_done(now_ms) {
            break;
        }
    }

    // Mark scroll as finished for adapters that use this state.
    v.set_is_scrolling(false);

    println!(
        "done: off={} range={:?}",
        v.scroll_offset(),
        v.virtual_range()
    );
}
