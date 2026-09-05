//! A shared animation primitive, used for every layout change that GTK3
//! doesn't animate for free: column resize, tab insert/remove, and scroll
//! position. (Workspace bloom/collapse uses `gtk::Stack`'s own built-in
//! transition instead -- see `workspace_slot.rs` -- since that's a native,
//! well-tested GTK mechanism for exactly that case.)
//!
//! GTK3's CSS `transition` only animates paint properties (color, opacity);
//! width/height/position need to be driven by hand, frame by frame, via
//! `gtk::Widget::add_tick_callback`. This is that driver, built once and
//! reused everywhere rather than four different ad-hoc animations.

use std::{cell::Cell, rc::Rc, time::Instant};

use waybar_cffi::gtk::{self, glib, prelude::WidgetExtManual};

/// Matches the transition duration already used in the existing waybar
/// CSS, so the animated version feels continuous with what's already
/// shipped rather than a different motion language bolted on.
const DURATION_MS: f64 = 150.0;

/// Close, cheap stand-in for `cubic-bezier(0.215, 0.61, 0.355, 1)` (a fast
/// start with a gentle settle) -- evaluated analytically instead of
/// needing a bezier root-solver on every frame.
fn ease_out_cubic(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

#[derive(Clone, Copy)]
struct State {
    start: f64,
    target: f64,
    start_time: Instant,
    running: bool,
}

/// Animates a single numeric value (a width, a scroll position, anything
/// `f64`-shaped) toward a target over `DURATION_MS`, via the widget's frame
/// clock. Create one `Animator` per animated property per widget and reuse
/// it -- calling [`Animator::to`] again mid-flight retargets from the
/// *current interpolated value*, not from the original start or the
/// previous target, and does not spawn a second competing tick callback.
pub struct Animator {
    state: Rc<Cell<State>>,
}

impl Animator {
    pub fn new(initial: f64) -> Self {
        Self {
            state: Rc::new(Cell::new(State {
                start: initial,
                target: initial,
                start_time: Instant::now(),
                running: false,
            })),
        }
    }

    /// The value as of right now: the live interpolated value while
    /// animating, or the settled target otherwise.
    pub fn current(&self) -> f64 {
        let s = self.state.get();
        if !s.running {
            return s.target;
        }
        let t = s.start_time.elapsed().as_secs_f64() * 1000.0 / DURATION_MS;
        if t >= 1.0 {
            s.target
        } else {
            s.start + (s.target - s.start) * ease_out_cubic(t)
        }
    }

    /// The animation's current destination, whether or not it's still in
    /// flight. Useful to skip redundant `to()` calls when the target
    /// hasn't actually changed.
    pub fn target(&self) -> f64 {
        self.state.get().target
    }

    /// Retargets the animation to `target`. `apply` runs on every frame
    /// with the interpolated value, and once more with the exact target
    /// value on completion. `widget` only provides the frame clock to
    /// drive the animation from -- it need not be the widget being resized,
    /// though in practice it always is.
    pub fn to(&self, widget: &gtk::Widget, target: f64, apply: impl Fn(f64) + 'static) {
        if !self.state.get().running && self.state.get().target == target {
            // Already settled at this exact target; nothing to do.
            return;
        }

        let now_value = self.current();
        let already_running = self.state.get().running;
        self.state.set(State {
            start: now_value,
            target,
            start_time: Instant::now(),
            running: true,
        });

        if already_running {
            // A tick callback is already driving this animator; it reads
            // `state` fresh every frame, so it'll pick up the retarget on
            // its next tick without us adding a second one.
            return;
        }

        // The tick callback outlives `self` (GTK may keep calling it for a
        // frame or two after the widget is scheduled for removal), so it
        // holds its own `Rc` clone of the shared state rather than
        // borrowing from `self` -- no unsafe, no dangling-pointer risk.
        let state = self.state.clone();
        widget.add_tick_callback(move |_widget, _clock| {
            let s = state.get();
            if !s.running {
                return glib::ControlFlow::Break;
            }
            let elapsed = s.start_time.elapsed().as_secs_f64() * 1000.0 / DURATION_MS;
            if elapsed >= 1.0 {
                apply(s.target);
                let mut done = s;
                done.running = false;
                state.set(done);
                glib::ControlFlow::Break
            } else {
                apply(s.start + (s.target - s.start) * ease_out_cubic(elapsed));
                glib::ControlFlow::Continue
            }
        });
    }
}
