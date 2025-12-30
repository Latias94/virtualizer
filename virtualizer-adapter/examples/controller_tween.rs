use virtualizer_adapter::{Controller, Easing};

fn main() {
    // Example: TanStack-like controller driving tween scrolling without holding any UI objects.
    //
    // An adapter would:
    // - start a tween (e.g. in response to "scroll to index" command)
    // - call tick(now_ms) in a frame loop / timer
    // - apply the returned offset to the real scroll container (if any)
    // - render using the virtualizer state
    let mut c = Controller::new(virtualizer::VirtualizerOptions::new(10_000, |_| 1));
    c.virtualizer_mut().set_viewport_size(20);
    c.virtualizer_mut().set_scroll_offset(0);

    let target = c.start_tween_to_index(
        2_000,
        virtualizer::Align::Center,
        0,
        240,
        Easing::SmoothStep,
    );
    println!("target_offset={target}");

    let mut now_ms = 0u64;
    loop {
        now_ms += 16;
        if let Some(off) = c.tick(now_ms) {
            if now_ms.is_multiple_of(80) {
                println!(
                    "t={now_ms} off={off} visible={:?}",
                    c.virtualizer().visible_range()
                );
            }
        } else {
            break;
        }
    }

    println!(
        "done: off={} range={:?}",
        c.virtualizer().scroll_offset(),
        c.virtualizer().virtual_range()
    );
}
