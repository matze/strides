//! Shared progress theme configuration.
//!
//! A [`Theme`] bundles a [`Spinner`] and a [`Bar`] together with an optional bar width
//! and is the value accepted by [`FutureExt`](crate::future::FutureExt),
//! [`StreamExt`](crate::stream::StreamExt) and [`Group`](crate::future::Group). A bare
//! [`Spinner`] also converts into a `Theme` via [`From`], so callers that only need a
//! spinner can pass one directly.
//!
//! ```rust
//! use strides::{bar, spinner, Theme};
//!
//! // Pair a spinner with a bar in one call.
//! let theme = Theme::with(spinner::styles::DOTS_3, bar::styles::SHADED);
//!
//! // Or build piece-by-piece when you need to set width or layout too.
//! let theme = Theme::new()
//!     .with_spinner(spinner::styles::DOTS_3)
//!     .with_bar(bar::styles::SHADED)
//!     .with_bar_width(40);
//! ```

use crate::bar::Bar;
use crate::layout::Layout;
use crate::spinner::{styles, Spinner};

/// Combined theme for progress display, bundling a [`Spinner`] and a [`Bar`].
#[derive(Clone)]
pub struct Theme<'a> {
    /// Spinner style to indicate activity.
    pub(crate) spinner: Spinner<'a>,
    /// Bar style to indicate progress.
    pub(crate) bar: Bar<'a>,
    /// Width of the progress bar in characters.
    pub(crate) bar_width: Option<usize>,
    /// Ordering and formatting of the rendered progress line.
    pub(crate) layout: Layout,
}

impl Default for Theme<'_> {
    /// A theme that animates out of the box: [`styles::DOTS_3`](crate::spinner::styles::DOTS_3)
    /// spinner, no bar, default [`Layout`]. For a fully empty theme to build on, use
    /// [`Theme::new`].
    fn default() -> Self {
        Self::new().with_spinner(styles::DOTS_3)
    }
}

impl<'a> Theme<'a> {
    /// Create an empty theme: inactive spinner, no bar, auto-detected bar width, default
    /// [`Layout`]. Most callers want [`Theme::default`] instead, which seeds a visible spinner;
    /// use `new` when building a theme bottom-up and explicitly setting every piece.
    pub const fn new() -> Self {
        Self {
            spinner: Spinner::inactive(),
            bar: Bar::empty(),
            bar_width: None,
            layout: Layout::DEFAULT,
        }
    }

    /// Create a theme that pairs `spinner` with `bar`. Sugar for
    /// `Theme::new().with_spinner(spinner).with_bar(bar)` — the spinner + bar combination is the
    /// overwhelmingly common case, and `Theme::with` names both at once instead of chaining.
    pub const fn with(spinner: Spinner<'a>, bar: Bar<'a>) -> Self {
        Self::new().with_spinner(spinner).with_bar(bar)
    }

    /// Set the [`Spinner`] used to indicate ongoing activity.
    pub const fn with_spinner(mut self, spinner: Spinner<'a>) -> Self {
        self.spinner = spinner;
        self
    }

    /// Set the [`Bar`] used to render fractional progress.
    pub const fn with_bar(mut self, bar: Bar<'a>) -> Self {
        self.bar = bar;
        self
    }

    /// Override the bar width in characters. When unset the width is derived from the terminal
    /// size (clamped to `10..=80`), falling back to `40` if the size cannot be detected.
    pub const fn with_bar_width(mut self, width: usize) -> Self {
        self.bar_width = Some(width);
        self
    }

    /// Set the [`Layout`] controlling segment order, spacing and per-segment formatting.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    pub(crate) fn effective_bar_width(&self) -> usize {
        self.bar_width.unwrap_or_else(|| {
            terminal_size::terminal_size()
                .map(|(w, _)| (w.0 as usize).saturating_sub(20).clamp(10, 80))
                .unwrap_or(40)
        })
    }
}

impl<'a> From<Spinner<'a>> for Theme<'a> {
    fn from(spinner: Spinner<'a>) -> Self {
        Theme::new().with_spinner(spinner)
    }
}
