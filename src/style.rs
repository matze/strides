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
}

impl<'a> ProgressStyle<'a> {
    pub fn new() -> Self {
        Self {
            spinner: Spinner::inactive(),
            bar: Bar::default(),
        }
    }

    pub fn with_spinner(mut self, spinner: Spinner<'a>) -> Self {
        self.spinner = spinner;
        self
    }

    pub fn with_bar(mut self, bar: Bar<'a>) -> Self {
        self.bar = bar;
        self
    }
}

impl<'a> From<Spinner<'a>> for ProgressStyle<'a> {
    fn from(spinner: Spinner<'a>) -> Self {
        ProgressStyle::new().with_spinner(spinner)
    }
}
