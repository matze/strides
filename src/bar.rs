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

use crate::color::{push_gradient_run, push_styled, Gradient};

/// How a [`Gradient`] is mapped across a bar's cells.
///
/// Picks what the gradient position `t` means for cell `i` of a run. [`Axis::Width`] is the
/// general case; [`Axis::Fraction`] reduces to a single solid color that depends on the fill
/// level (a gauge).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    /// `t = position / (width - 1)`: colors are fixed to absolute column, so the gradient stays
    /// put and the fill reveals more of it as it grows.
    Width,
    /// `t = fill fraction`: every cell shares one color derived from how full the bar is, e.g.
    /// green when nearly empty shading to red when full.
    Fraction,
}

impl Axis {
    /// Gradient position for cell `i` of a run starting at absolute column `offset` in a bar of
    /// `width` columns whose fill fraction is `fraction`.
    fn position(self, i: usize, offset: usize, width: usize, fraction: f64) -> f64 {
        match self {
            Axis::Fraction => fraction,
            Axis::Width => {
                let denom = width.saturating_sub(1).max(1) as f64;
                (offset + i) as f64 / denom
            }
        }
    }
}

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
pub struct Bar {
    /// Character to symbolize incompleteness.
    empty: Option<char>,
    /// Character to symbolize completeness.
    complete: Option<char>,
    /// Characters in between complete and incomplete.
    in_between: Option<&'static str>,
    /// Left border character
    left_border: Option<&'static str>,
    /// Right border character
    right_border: Option<&'static str>,
    /// Style applied to the filled portion of the bar.
    filled_style: Option<owo_colors::Style>,
    /// Style applied to the empty portion of the bar.
    empty_style: Option<owo_colors::Style>,
    /// Gradient and mapping for the filled portion; takes precedence over `filled_style`.
    filled_gradient: Option<(Gradient, Axis)>,
    /// Gradient and mapping for the empty portion; takes precedence over `empty_style`.
    empty_gradient: Option<(Gradient, Axis)>,
}

impl Bar {
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
            filled_gradient: None,
            empty_gradient: None,
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
            filled_gradient: None,
            empty_gradient: None,
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
        let fraction = completed.clamp(0.0, 1.0);
        let filled = (fraction * width as f64) as usize;
        let remaining = width.saturating_sub(filled);

        if let Some(left) = self.left_border {
            buf.push_str(left);
        }

        if let Some(c) = self.complete {
            // The filled run occupies columns `0..filled`.
            self.render_run(
                buf,
                c,
                filled,
                0,
                width,
                fraction,
                self.filled_gradient,
                self.filled_style,
            );
        }

        if let Some(in_between) = self.in_between {
            buf.push_str(in_between);
        }

        if let Some(c) = self.empty {
            // The empty run continues at column `filled`.
            self.render_run(
                buf,
                c,
                remaining,
                filled,
                width,
                fraction,
                self.empty_gradient,
                self.empty_style,
            );
        }

        if let Some(right) = self.right_border {
            buf.push_str(right);
        }
    }

    /// Render one run of `count` copies of `ch` starting at absolute column `offset`. A gradient,
    /// when present, colors per cell and takes precedence over a solid `style`.
    #[allow(clippy::too_many_arguments)]
    fn render_run(
        &self,
        buf: &mut String,
        ch: char,
        count: usize,
        offset: usize,
        width: usize,
        fraction: f64,
        gradient: Option<(Gradient, Axis)>,
        style: Option<owo_colors::Style>,
    ) {
        match gradient {
            Some((gradient, axis)) => {
                push_gradient_run(buf, ch, count, |i| {
                    gradient.sample(axis.position(i, offset, width, fraction))
                });
            }
            None => {
                let run = CharRun::new(ch, count);
                match style {
                    Some(style) => push_styled(buf, run, style),
                    None => {
                        let _ = write!(buf, "{run}");
                    }
                }
            }
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

    /// Color the filled portion with a [`Gradient`], mapped across the bar by `axis`. Takes
    /// precedence over [`with_filled_style`](Self::with_filled_style) when both are set. Output
    /// downgrades to a 256-color approximation or to no color depending on terminal support.
    pub const fn with_filled_gradient(mut self, gradient: Gradient, axis: Axis) -> Self {
        self.filled_gradient = Some((gradient, axis));
        self
    }

    /// Color the empty portion with a [`Gradient`], mapped across the bar by `axis`. Takes
    /// precedence over [`with_empty_style`](Self::with_empty_style) when both are set.
    pub const fn with_empty_gradient(mut self, gradient: Gradient, axis: Axis) -> Self {
        self.empty_gradient = Some((gradient, axis));
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
