use crate::*;

use std::collections::HashMap;

#[test]
fn anchor_can_preserve_scroll_across_prepend() {
    let mut v1 = virtualizer::Virtualizer::new(virtualizer::VirtualizerOptions::new_with_key(
        100,
        |_| 1,
        |i| 1000u64 + i as u64,
    ));
    v1.set_viewport_and_scroll_clamped(10, 50);

    let anchor = capture_first_visible_anchor(&v1).unwrap();
    assert_eq!(anchor.key, 1050);
    assert_eq!(anchor.offset_in_viewport, 0);

    // Prepend 10 items: old items shift by +10 indexes.
    let mut v2 = virtualizer::Virtualizer::new(virtualizer::VirtualizerOptions::new_with_key(
        110,
        |_| 1,
        |i| {
            if i < 10 {
                2000u64 + i as u64
            } else {
                1000u64 + (i - 10) as u64
            }
        },
    ));
    v2.set_viewport_and_scroll_clamped(10, 50);

    let mut map = HashMap::<u64, usize>::new();
    for i in 0..110usize {
        map.insert(v2.key_for(i), i);
    }

    assert!(apply_anchor(&mut v2, &anchor, |k| map.get(k).copied()));
    assert_eq!(v2.scroll_offset(), 60);
}

#[test]
fn controller_tween_drives_scroll_offset() {
    let mut c = Controller::new(virtualizer::VirtualizerOptions::new(1000, |_| 1));
    c.virtualizer_mut().set_viewport_size(10);
    c.virtualizer_mut().set_scroll_offset(0);

    let to = c.start_tween_to_index(500, virtualizer::Align::Start, 0, 100, Easing::SmoothStep);
    assert!(to > 0);

    let mut last = 0u64;
    for now_ms in [0u64, 10, 20, 40, 80, 100, 120] {
        if let Some(off) = c.tick(now_ms) {
            assert!(off >= last);
            last = off;
        }
    }
    assert!(!c.is_animating());
    assert_eq!(c.virtualizer().scroll_offset(), to);
}
