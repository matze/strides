//! Progress bar UI element.
//!
//! A [`Bar`] is a pair of characters (one for the filled portion, one for the empty portion)
//! together with optional borders, an in-between separator, and per-portion styling.
//! Pre-defined variants live in the [`styles`] module.
//!
//! Bars are composed into a [`Theme`](crate::Theme) and consumed by
//! [`FutureExt`](crate::future::FutureExt) and [`StreamExt`](crate::stream::StreamExt).
//!
//! ```rust
//! use strides::bar;
//!
//! let bar = bar::styles::THIN_LINE
//!     .with_border("[", "]")
//!     .with_filled_style(owo_colors::Style::new().bright_purple());
//! ```

use std::fmt::{self, Display, Formatter, Write as _};

use owo_colors::OwoColorize;

/// Pre-defined progress bar styles. Each constant is a ready-to-use [`Bar`] that can be
/// further customized with the builder methods on [`Bar`].
pub mod styles {
    use super::Bar;

    /// Parallelogram blocks: `▱▱▱▰▰▰`.
    pub const PARALLELOGRAM: Bar = Bar::new('▱', '▰');

    /// Light shading to full block: `░░░███`.
    pub const SHADED: Bar = Bar::new('░', '█');

    /// Light to medium shading: `░░░▒▒▒`.
    pub const MEDIUM_SHADED: Bar = Bar::new('░', '▒');

    /// Medium to dark shading: `▒▒▒▓▓▓`.
    pub const HEAVY_SHADED: Bar = Bar::new('▒', '▓');

    /// Braille dots: `⣀⣀⣀⣿⣿⣿`.
    pub const DOTTED: Bar = Bar::new('⣀', '⣿');

    /// Thin to thick horizontal line: `───━━━`.
    pub const THIN_LINE: Bar = Bar::new('─', '━');

    /// Triple dash, light to heavy: `┄┄┄┅┅┅`.
    pub const TRIPLE_DASH: Bar = Bar::new('┄', '┅');

    /// Middle dots, small to large: `···•••`.
    pub const MID_DOTS: Bar = Bar::new('·', '•');

    /// Dashed to equals sign: `╌╌╌═══`.
    pub const EQUALS: Bar = Bar::new('╌', '═');
}

/// Progress bar style characters.
#[derive(Default, Clone)]
pub struct Bar<'a> {
    /// Character to symbolize incompleteness.
    empty: Option<char>,
    /// Character to symbolize completeness.
    complete: Option<char>,
    /// Characters in between complete and incomplete.
    in_between: Option<&'a str>,
    /// Left border character
    left_border: Option<&'a str>,
    /// Right border character
    right_border: Option<&'a str>,
    /// Style applied to the filled portion of the bar.
    filled_style: Option<owo_colors::Style>,
    /// Style applied to the empty portion of the bar.
    empty_style: Option<owo_colors::Style>,
}

impl<'a> Bar<'a> {
    /// Create a new bar from the character used for the empty portion and the character used for
    /// the filled portion. See the [`styles`] module for pre-defined variants.
    pub const fn new(empty: char, complete: char) -> Self {
        Self {
            empty: Some(empty),
            complete: Some(complete),
            in_between: None,
            left_border: None,
            right_border: None,
            filled_style: None,
            empty_style: None,
        }
    }

    pub(crate) const fn empty() -> Self {
        Self {
            empty: None,
            complete: None,
            in_between: None,
            left_border: None,
            right_border: None,
            filled_style: None,
            empty_style: None,
        }
    }

    /// Render the bar to a string of the given character `width` with `completed` interpreted as
    /// a fraction in `0.0..=1.0`. Borders and the in-between separator are added outside that
    /// width. Mostly useful when integrating the bar into a custom renderer.
    pub fn render(&self, width: usize, completed: f64) -> String {
        let mut buf = String::new();
        self.render_into(&mut buf, width, completed);
        buf
    }

    /// Like [`render`](Self::render) but appends into `buf` instead of allocating a new `String`.
    /// Use this on hot paths where the same buffer can be cleared and reused across frames.
    pub fn render_into(&self, buf: &mut String, width: usize, completed: f64) {
        let completed = (completed * width as f64) as usize;
        let remaining = width.saturating_sub(completed);

        if let Some(left) = self.left_border {
            buf.push_str(left);
        }

        if let Some(c) = self.complete {
            let run = CharRun::new(c, completed);
            match self.filled_style {
                Some(style) => { let _ = write!(buf, "{}", run.style(style)); }
                None => { let _ = write!(buf, "{run}"); }
            }
        }

        if let Some(in_between) = self.in_between {
            buf.push_str(in_between);
        }

        if let Some(c) = self.empty {
            let run = CharRun::new(c, remaining);
            match self.empty_style {
                Some(style) => { let _ = write!(buf, "{}", run.style(style)); }
                None => { let _ = write!(buf, "{run}"); }
            }
        }

        if let Some(right) = self.right_border {
            buf.push_str(right);
        }
    }

    /// Insert `chars` between the filled and empty portions, useful for a tip character such as
    /// `>` or `▶`.
    pub const fn with_in_between(mut self, chars: &'static str) -> Self {
        self.in_between = Some(chars);
        self
    }

    /// Wrap the bar with `left` and `right` border strings, for example `"["` and `"]"`.
    pub const fn with_border(mut self, left: &'static str, right: &'static str) -> Self {
        self.left_border = Some(left);
        self.right_border = Some(right);
        self
    }

    /// Style the filled portion of the bar.
    pub const fn with_filled_style(mut self, style: owo_colors::Style) -> Self {
        self.filled_style = Some(style);
        self
    }

    /// Style the empty portion of the bar.
    pub const fn with_empty_style(mut self, style: owo_colors::Style) -> Self {
        self.empty_style = Some(style);
        self
    }
}

/// A repeated character rendered through `Display` so the styling layer can wrap it without
/// materializing the intermediate run as a `String`.
struct CharRun {
    ch: char,
    count: usize,
}

impl CharRun {
    const fn new(ch: char, count: usize) -> Self {
        Self { ch, count }
    }
}

impl Display for CharRun {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for _ in 0..self.count {
            f.write_char(self.ch)?;
        }
        Ok(())
    }
}
