//! Shared progress style configuration.

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

    pub const fn with_spinner(mut self, spinner: Spinner<'a>) -> Self {
        self.spinner = spinner;
        self
    }

    pub const fn with_bar(mut self, bar: Bar<'a>) -> Self {
        self.bar = bar;
        self
    }

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
