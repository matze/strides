//! Spinner UI element.

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
}

/// A stream of spinner characters emitted at a set interval.
pub struct Ticks<'a> {
    /// All characters to cycle through.
    all_chars: &'a str,
    /// Iterator over the current cycle.
    chars: std::str::Chars<'a>,
    /// One-shot delay that is reset after each tick.
    delay: Delay,
    /// Interval between ticks.
    interval: Duration,
}

impl Stream for Ticks<'_> {
    type Item = char;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<char>> {
        let this = self.get_mut();

        // Wait for the current delay to expire.
        match Pin::new(&mut this.delay).poll(cx) {
            Poll::Ready(()) => {
                this.delay.reset(this.interval);
                let _ = Pin::new(&mut this.delay).poll(cx);
            }
            Poll::Pending => return Poll::Pending,
        }

        // Get the next character, cycling back to the start when exhausted.
        let ch = match this.chars.next() {
            Some(ch) => ch,
            None => {
                this.chars = this.all_chars.chars();
                this.chars.next().expect("non-empty spinner chars")
            }
        };

        Poll::Ready(Some(ch))
    }
}

/// A spinner that emits a character at a set interval.
#[derive(Clone)]
pub struct Spinner<'a> {
    /// Characters making up the spinner.
    chars: &'a str,
    /// Refresh interval.
    interval: Duration,
}

impl<'a> Spinner<'a> {
    /// Create a new spinner with `chars`. See the [`styles`] module for pre-defined styles.
    pub const fn new(chars: &'a str) -> Self {
        Self {
            chars,
            interval: Duration::from_millis(80),
        }
    }

    /// Create an inactive spinner that will not emit a character.
    pub const fn inactive() -> Self {
        Self {
            chars: "",
            interval: Duration::MAX,
        }
    }

    /// Set an animation interval different from the default.
    pub const fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Return a stream of characters at the set interval.
    pub fn ticks(&self) -> Ticks<'a> {
        Ticks {
            all_chars: self.chars,
            chars: self.chars.chars(),
            delay: Delay::new(self.interval),
            interval: self.interval,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    use futures_lite::{StreamExt, future};

    #[test]
    fn spinner() {
        let interval = Duration::from_millis(20);
        let spinner = styles::DOTS_3.with_interval(interval);
        let num = spinner.chars.chars().count();
        let ticks = spinner.ticks();

        future::block_on(async move {
            let start = Instant::now();
            let ticks = ticks.take(num + 1).collect::<Vec<_>>().await;
            let elapsed = start.elapsed();
            let at_least = interval.saturating_mul(num as u32 + 1);
            assert!(elapsed >= at_least);
            assert_eq!(ticks[..num], spinner.chars.chars().collect::<Vec<_>>());
            assert_eq!(ticks[0], ticks[num]);
        });
    }
}
