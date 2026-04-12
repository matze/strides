//! Spinner UI element.

use std::time::Duration;

use futures_lite::{Stream, StreamExt, stream};

/// Pre-defined spinner styles.
pub mod styles {
    use super::Spinner;

    /// Segment of a circle circling: `◜◝◞◟`.
    pub const CIRCLE: Spinner = Spinner::new("◜◝◞◟");

    /// Three dots circling: `⠖⠲⠴⠦`.
    pub const DOTS_3: Spinner = Spinner::new("⠖⠲⠴⠦");

    /// Seven dots circling: `⣾⣽⣻⢿⡿⣟⣯⣷`.
    pub const DOTS_7: Spinner = Spinner::new("⣾⣽⣻⢿⡿⣟⣯⣷");

    /// One dot circling in a large square: `⠁⠂⠄⡀⢀⠠⠐⠈`.
    pub const DOT_LARGE_SQUARE: Spinner = Spinner::new("⠁⠂⠄⡀⢀⠠⠐⠈");

    /// Falling sand: `⠁⠂⠄⡀⡈⡐⡠⣀⣁⣂⣄⣌⣔⣤⣥⣦⣮⣶⣷⣿⡿⠿⢟⠟⡛⠛⠫⢋⠋⠍⡉⠉⠑⠡⢁`.
    pub const SAND: Spinner = Spinner::new("⠁⠂⠄⡀⡈⡐⡠⣀⣁⣂⣄⣌⣔⣤⣥⣦⣮⣶⣷⣿⡿⠿⢟⠟⡛⠛⠫⢋⠋⠍⡉⠉⠑⠡⢁");
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
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Return a stream of characters at the set interval.
    pub fn ticks(&self) -> impl Stream<Item = char> + use<'a> {
        stream::iter(self.chars.chars())
            .cycle()
            .zip(async_io::Timer::interval(self.interval))
            .map(|(spinner, _)| spinner)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    use futures_lite::future;

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
