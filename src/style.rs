//! Shared progress style configuration.
//!
//! A [`ProgressStyle`] bundles a [`Spinner`] and a [`Bar`] together with an optional bar width
//! and is the value accepted by [`FutureExt`](crate::future::FutureExt),
//! [`StreamExt`](crate::stream::StreamExt) and [`Group`](crate::future::Group). A bare
//! [`Spinner`] also converts into a `ProgressStyle` via [`From`], so callers that only need a
//! spinner can pass one directly.
//!
//! ```rust
//! use strides::{bar, spinner, style::ProgressStyle};
//!
//! let style = ProgressStyle::new()
//!     .with_spinner(spinner::styles::DOTS_3)
//!     .with_bar(bar::styles::SHADED)
//!     .with_bar_width(40);
//! ```

use crate::bar::Bar;
use crate::spinner::Spinner;

/// Combined style for progress display, bundling a [`Spinner`] and a [`Bar`].
#[derive(Clone)]
pub struct ProgressStyle<'a> {
    /// Spinner style to indicate activity.
    pub(crate) spinner: Spinner<'a>,
    /// Bar style to indicate progress.
    pub(crate) bar: Bar<'a>,
    /// Width of the progress bar in characters.
    pub(crate) bar_width: Option<usize>,
}

impl<'a> Default for ProgressStyle<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> ProgressStyle<'a> {
    /// Create a new progress style with an inactive spinner, no bar, and an
    /// auto-detected bar width.
    pub const fn new() -> Self {
        Self {
            spinner: Spinner::inactive(),
            bar: Bar::empty(),
            bar_width: None,
        }
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

    pub(crate) fn effective_bar_width(&self) -> usize {
        self.bar_width.unwrap_or_else(|| {
            terminal_size::terminal_size()
                .map(|(w, _)| (w.0 as usize).saturating_sub(20).clamp(10, 80))
                .unwrap_or(40)
        })
    }
}

impl<'a> From<Spinner<'a>> for ProgressStyle<'a> {
    fn from(spinner: Spinner<'a>) -> Self {
        ProgressStyle::default().with_spinner(spinner)
    }
}
