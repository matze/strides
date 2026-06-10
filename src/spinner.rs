//! Spinner UI element.
//!
//! A [`Spinner`] holds a sequence of animation frames and a tick interval. Calling
//! [`ticks()`](Spinner::ticks) returns a [`Stream`] that yields the next frame every
//! interval, cycling forever. Pre-defined variants live in the [`styles`] module.
//!
//! A frame is a string slice. The common single-glyph case is built with
//! [`Spinner::new`], where each *character* of the passed string is one frame:
//!
//! ```rust
//! use std::time::Duration;
//! use futures_lite::{StreamExt, future};
//! use strides::spinner;
//!
//! let custom = spinner::Spinner::new("◐◓◑◒").with_interval(Duration::from_millis(120));
//!
//! # future::block_on(async {
//! let first: Vec<&str> = custom.ticks().take(4).collect().await;
//! assert_eq!(first, vec!["◐", "◓", "◑", "◒"]);
//! # });
//! ```
//!
//! For multi-cell animations such as a Knight Rider / K.I.T.T. scanner band, where each
//! frame spans several columns, build the spinner from explicit frames with
//! [`Spinner::frames`]. All frames should share the same display width so the line does
//! not jitter as it animates:
//!
//! ```rust
//! use futures_lite::{StreamExt, future};
//! use strides::spinner;
//!
//! let kitt = spinner::Spinner::frames(&["▰▱▱", "▱▰▱", "▱▱▰"]);
//!
//! # future::block_on(async {
//! let first: Vec<&str> = kitt.ticks().take(3).collect().await;
//! assert_eq!(first, vec!["▰▱▱", "▱▰▱", "▱▱▰"]);
//! # });
//! ```

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_lite::Stream;
use futures_timer::Delay;

/// Pre-defined spinner styles.
pub mod styles {
    use super::Spinner;

    /// Arc segment circling: `◜◝◞◟`.
    pub const ARC: Spinner = Spinner::new("◜◝◞◟");

    /// Braille dots: `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`.
    pub const DOTS: Spinner = Spinner::new("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏");

    /// Braille dots variant 2: `⠋⠙⠚⠞⠖⠦⠴⠲⠳⠓`.
    pub const DOTS_2: Spinner = Spinner::new("⠋⠙⠚⠞⠖⠦⠴⠲⠳⠓");

    /// Three braille dots circling: `⠖⠲⠴⠦`.
    pub const DOTS_3: Spinner = Spinner::new("⠖⠲⠴⠦");

    /// Braille dots bouncing: `⠄⠆⠇⠋⠙⠸⠰⠠⠰⠸⠙⠋⠇⠆`.
    pub const DOTS_4: Spinner = Spinner::new("⠄⠆⠇⠋⠙⠸⠰⠠⠰⠸⠙⠋⠇⠆");

    /// Braille dots wave: `⠋⠙⠚⠒⠂⠂⠒⠲⠴⠦⠖⠒⠐⠐⠒⠓`.
    pub const DOTS_5: Spinner = Spinner::new("⠋⠙⠚⠒⠂⠂⠒⠲⠴⠦⠖⠒⠐⠐⠒⠓");

    /// Braille dots breathing: `⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠴⠲⠒⠂⠂⠒⠚⠙⠉`.
    pub const DOTS_6: Spinner = Spinner::new("⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠴⠲⠒⠂⠂⠒⠚⠙⠉");

    /// Seven braille dots circling: `⣾⣽⣻⢿⡿⣟⣯⣷`.
    pub const DOTS_7: Spinner = Spinner::new("⣾⣽⣻⢿⡿⣟⣯⣷");

    /// Braille dots pulsing: `⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈`.
    pub const DOTS_8: Spinner = Spinner::new("⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈");

    /// Two braille dots circling: `⠃⠉⠘⠰⢠⣀⡄⠆`.
    pub const DOTS_CIRCLE: Spinner = Spinner::new("⠃⠉⠘⠰⢠⣀⡄⠆");

    /// One dot circling in a large square: `⠁⠂⠄⡀⢀⠠⠐⠈`.
    pub const DOT_LARGE_SQUARE: Spinner = Spinner::new("⠁⠂⠄⡀⢀⠠⠐⠈");

    /// Star: `✶✸✹✺✹✷`.
    pub const STAR: Spinner = Spinner::new("✶✸✹✺✹✷");

    /// Falling sand: `⠁⠂⠄⡀⡈⡐⡠⣀⣁⣂⣄⣌⣔⣤⣥⣦⣮⣶⣷⣿⡿⠿⢟⠟⡛⠛⠫⢋⠋⠍⡉⠉⠑⠡⢁`.
    pub const SAND: Spinner = Spinner::new("⠁⠂⠄⡀⡈⡐⡠⣀⣁⣂⣄⣌⣔⣤⣥⣦⣮⣶⣷⣿⡿⠿⢟⠟⡛⠛⠫⢋⠋⠍⡉⠉⠑⠡⢁");

    /// Knight Rider / K.I.T.T. scanner: a single lit cell `▰` sweeping a six-cell track of dim
    /// `▱` and bouncing back.
    pub const KNIGHT: Spinner = Spinner::frames(&[
        "▰▱▱▱▱▱",
        "▱▰▱▱▱▱",
        "▱▱▰▱▱▱",
        "▱▱▱▰▱▱",
        "▱▱▱▱▰▱",
        "▱▱▱▱▱▰",
        "▱▱▱▱▰▱",
        "▱▱▱▰▱▱",
        "▱▱▰▱▱▱",
        "▱▰▱▱▱▱",
    ]);

    /// Knight Rider with a fading comet trail (`█` head, `▓▒` tail) sweeping a six-cell track and
    /// bouncing back.
    pub const KNIGHT_COMET: Spinner = Spinner::frames(&[
        "█     ",
        "▓█    ",
        "▒▓█   ",
        " ▒▓█  ",
        "  ▒▓█ ",
        "   ▒▓█",
        "    █▓",
        "   █▓▒",
        "  █▓▒ ",
        " █▓▒  ",
    ]);

    /// A ball `●` bouncing back and forth.
    pub const BOUNCE: Spinner = Spinner::frames(&[
        "●    ", " ●   ", "  ●  ", "   ● ", "    ●", "   ● ", "  ●  ", " ●   ",
    ]);

    /// A block sliding back and forth through a dotted track: `█░░░` … `░░░█` and back.
    pub const BAR: Spinner = Spinner::frames(&["█░░░", "░█░░", "░░█░", "░░░█", "░░█░", "░█░░"]);

    /// A centred thin line breathing in and out: `──` → `────` → `──────` → `────`.
    pub const PULSE: Spinner = Spinner::frames(&["  ──  ", " ──── ", "──────", " ──── "]);
}

/// The source of a spinner's animation frames.
///
/// Either each `char` of a single string is a frame (the common single-glyph case built with
/// [`Spinner::new`]) or each `&str` of a slice is a frame (multi-cell animations built with
/// [`Spinner::frames`]). Frame data is `&'static` so spinners, themes and the adapters built from
/// them carry no lifetime parameter; frames computed at runtime can be promoted with
/// [`String::leak`] / [`Vec::leak`].
#[derive(Clone, Copy)]
enum Frames {
    /// Each `char` of the string is one frame.
    Chars(&'static str),
    /// Each `&str` of the slice is one frame.
    Strs(&'static [&'static str]),
}

impl Frames {
    /// Whether there are no frames at all, the sentinel for an inactive spinner.
    const fn is_empty(&self) -> bool {
        match self {
            Frames::Chars(s) => s.is_empty(),
            Frames::Strs(f) => f.is_empty(),
        }
    }

    /// Number of frames in one cycle.
    fn len(&self) -> usize {
        match self {
            Frames::Chars(s) => s.chars().count(),
            Frames::Strs(f) => f.len(),
        }
    }

    /// The frame at `index`, or `None` when out of range.
    fn get(&self, index: usize) -> Option<&'static str> {
        match self {
            Frames::Chars(s) => {
                let (start, ch) = s.char_indices().nth(index)?;
                Some(&s[start..start + ch.len_utf8()])
            }
            Frames::Strs(f) => f.get(index).copied(),
        }
    }
}

/// A stream of spinner frames emitted at a set interval.
pub struct Ticks {
    /// All frames to cycle through.
    frames: Frames,
    /// Index of the next frame to yield.
    next: usize,
    /// One-shot delay that is reset after each tick. `None` for a never-yielding stream.
    delay: Option<Delay>,
    /// Interval between ticks.
    interval: Duration,
}

impl Ticks {
    /// A stream that neither yields a frame nor arms a timer. Used when there is no spinner to
    /// animate or the output is not a terminal, so polling never schedules a wakeup.
    pub(crate) const fn never() -> Self {
        Self {
            frames: Frames::Chars(""),
            next: 0,
            delay: None,
            interval: Duration::MAX,
        }
    }
}

impl Stream for Ticks {
    type Item = &'static str;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<&'static str>> {
        let this = self.get_mut();

        // A never-yielding stream carries no delay.
        let Some(delay) = &mut this.delay else {
            return Poll::Pending;
        };

        // Wait for the current delay to expire.
        match Pin::new(&mut *delay).poll(cx) {
            Poll::Ready(()) => {
                delay.reset(this.interval);
                let _ = Pin::new(delay).poll(cx);
            }
            Poll::Pending => return Poll::Pending,
        }

        // Get the next frame, cycling back to the start when exhausted.
        let count = this.frames.len();
        let frame = this
            .frames
            .get(this.next)
            .expect("index kept in range below");
        this.next = (this.next + 1) % count;

        Poll::Ready(Some(frame))
    }
}

/// A spinner that emits a frame at a set interval.
#[derive(Clone)]
pub struct Spinner {
    /// Frames making up the spinner.
    frames: Frames,
    /// Refresh interval.
    interval: Duration,
}

impl Spinner {
    /// Create a spinner whose frames are the individual characters of `chars`. This is the
    /// ergonomic constructor for single-glyph spinners; see the [`styles`] module for pre-defined
    /// styles. For multi-cell animations use [`Spinner::frames`]. Frames built at runtime can be
    /// promoted to `&'static str` with [`String::leak`].
    pub const fn new(chars: &'static str) -> Self {
        Self {
            frames: Frames::Chars(chars),
            interval: Duration::from_millis(80),
        }
    }

    /// Create a spinner from explicit multi-character `frames`, one `&str` per frame. Use this for
    /// animations whose frames span several columns, such as a Knight Rider / K.I.T.T. band. All
    /// frames should share the same display width so the rendered line does not jitter.
    pub const fn frames(frames: &'static [&'static str]) -> Self {
        Self {
            frames: Frames::Strs(frames),
            interval: Duration::from_millis(80),
        }
    }

    /// Set an animation interval different from the default.
    pub const fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Return a stream of frames at the set interval.
    pub fn ticks(&self) -> Ticks {
        // A spinner without frames carries no delay; `poll_next` short-circuits to `Pending` so
        // it never yields and `Instant + interval` is never computed.
        let delay = (!self.frames.is_empty()).then(|| Delay::new(self.interval));
        Ticks {
            frames: self.frames,
            next: 0,
            delay,
            interval: self.interval,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    use futures_lite::{future, StreamExt};

    #[test]
    fn spinner() {
        let interval = Duration::from_millis(20);
        let spinner = styles::DOTS_3.with_interval(interval);
        let num = spinner.frames.len();
        let ticks = spinner.ticks();

        future::block_on(async move {
            let start = Instant::now();
            let ticks = ticks.take(num + 1).collect::<Vec<_>>().await;
            let elapsed = start.elapsed();
            let at_least = interval.saturating_mul(num as u32 + 1);
            assert!(elapsed >= at_least);
            // Compare against the per-character frames of the underlying string.
            let Frames::Chars(s) = spinner.frames else {
                unreachable!("DOTS_3 is a char spinner");
            };
            let expected = s.char_indices().map(|(i, c)| &s[i..i + c.len_utf8()]);
            assert!(ticks[..num].iter().copied().eq(expected));
            assert_eq!(ticks[0], ticks[num]);
        });
    }

    #[test]
    fn multi_cell_frames() {
        let interval = Duration::from_millis(5);
        let spinner = styles::KNIGHT.with_interval(interval);
        let num = spinner.frames.len();
        let Frames::Strs(expected) = spinner.frames else {
            unreachable!("KNIGHT is a frame spinner");
        };

        future::block_on(async move {
            let ticks = spinner.ticks().take(num + 1).collect::<Vec<_>>().await;
            assert_eq!(&ticks[..num], expected);
            // Each frame is a multi-cell string and the cycle wraps.
            assert!(ticks[0].chars().count() > 1);
            assert_eq!(ticks[0], ticks[num]);
        });
    }
}
