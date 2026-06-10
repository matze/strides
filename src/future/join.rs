//! Drive N concurrent futures and render them as a single progress line.
//!
//! [`Join`] owns its inner futures and polls them concurrently. Its [`Progressive::progress`] is
//! the completion fraction (`completed / total`), so the bar fills from 0/N to N/N as each future
//! resolves. Push one into a [`Group`](super::Group) to render many futures as one line alongside
//! other independent rows, or call [`with_theme`](Join::with_theme) for a self-contained row.

use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_lite::stream::Pending;
use futures_lite::{stream, Stream};
use owo_colors::Style;
use pin_project_lite::pin_project;

use crate::progress::Progress;
use crate::progressive::Progressive;
use crate::Theme;

pin_project! {
    /// N concurrent futures collapsed into a single [`Progressive`] row.
    ///
    /// The bar fills from 0/N to N/N as each inner future completes. Results are collected in
    /// completion order. With zero inputs, awaiting resolves immediately with an empty `Vec` and no
    /// progress is reported.
    pub struct Join<F: Future, M = Pending<&'static str>> {
        futs: Vec<Pin<Box<F>>>,
        results: Vec<F::Output>,
        completed: usize,
        total: usize,
        #[pin]
        messages: M,
        core: Progress,
    }
}

/// Construct a [`Join`] from an iterable of futures sharing an `Output` type.
///
/// Accepts any [`IntoIterator`] of futures — `Vec<F>`, arrays, or any adapter chain. Without
/// [`with_theme`](Join::with_theme) the result inherits the parent `Group`'s theme when pushed,
/// with `with_theme` it renders standalone or overrides the Group's theme per-row.
pub fn join<I>(futs: I) -> Join<I::Item>
where
    I: IntoIterator,
    I::Item: Future,
{
    Join::new(futs)
}

impl<F: Future> Join<F> {
    /// Construct a `Join` with no theme set. Awaited directly it renders with [`Theme::default()`];
    /// chain [`with_theme`](Self::with_theme) for a custom theme, or push it into a
    /// [`Group`](super::Group) to inherit the Group's theme.
    pub fn new<I>(futs: I) -> Self
    where
        I: IntoIterator<Item = F>,
    {
        let futs: Vec<Pin<Box<F>>> = futs.into_iter().map(Box::pin).collect();
        let total = futs.len();
        Self {
            results: Vec::with_capacity(total),
            futs,
            completed: 0,
            total,
            messages: stream::pending(),
            core: Progress::new(),
        }
    }
}

impl<F: Future, M> Join<F, M> {
    /// Render this row with `theme`. Used for both the standalone path (drives the spinner /
    /// bar / cursor on its own line when awaited) and the per-row override path inside a
    /// [`Group`](super::Group).
    pub fn with_theme(mut self, theme: impl Into<Theme>) -> Self {
        self.core.set_theme(theme.into());
        self
    }

    /// Apply `style` to the spinner character on this row, overriding the parent Group's default.
    pub fn with_spinner_style(mut self, style: Style) -> Self {
        self.core.set_spinner_style(style);
        self
    }

    /// Apply `style` to the annotation (label) text on this row, overriding the parent Group's
    /// default.
    pub fn with_annotation_style(mut self, style: Style) -> Self {
        self.core.set_annotation_style(style);
        self
    }

    /// Set the static label shown in the [`Label`](crate::layout::Segment::Label) segment.
    /// `&'static str` and `String` convert zero-copy; formatted values should be `format!`'d at
    /// the call site.
    pub fn with_label(mut self, label: impl Into<Cow<'static, str>>) -> Self {
        self.core.set_label(label.into());
        self
    }

    /// Prepend the elapsed time to the line.
    pub fn with_elapsed_time(mut self) -> Self {
        self.core.enable_elapsed_time();
        self
    }

    /// Replace the displayed message each time `messages` yields a value. The item type is
    /// anything that converts into a `Cow<'static, str>`: `&'static str` and `String` are
    /// zero-copy; other formatted values should be `format!`'d at the call site.
    pub fn with_messages<S>(self, messages: S) -> Join<F, S>
    where
        S: Stream,
        S::Item: Into<Cow<'static, str>>,
    {
        Join {
            futs: self.futs,
            results: self.results,
            completed: self.completed,
            total: self.total,
            messages,
            core: self.core,
        }
    }
}

impl<F: Future, M> Progressive for Join<F, M> {
    fn label(&self) -> Option<&str> {
        self.core.label()
    }

    fn message(&self) -> Option<&str> {
        self.core.message()
    }

    fn progress(&self) -> Option<f64> {
        if self.total == 0 {
            None
        } else {
            Some(self.completed as f64 / self.total as f64)
        }
    }

    fn detach_rendering(&mut self) {
        self.core.detach_rendering();
    }

    fn theme(&self) -> Option<&Theme> {
        self.core.theme()
    }

    fn spinner_style(&self) -> Option<Style> {
        self.core.spinner_style()
    }

    fn annotation_style(&self) -> Option<Style> {
        self.core.annotation_style()
    }

    fn show_elapsed_time(&self) -> bool {
        self.core.show_elapsed_time()
    }
}

impl<F, M> Future for Join<F, M>
where
    F: Future,
    M: Stream,
    M::Item: Into<Cow<'static, str>>,
{
    type Output = Vec<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        if this.core.materialize() && *this.total > 0 {
            // `standalone_render` reads progress from `State`, but `Join`'s `progress()` is
            // derived from `completed / total`. Mirror it into state so the bar renders.
            this.core
                .state
                .set_progress(*this.completed as f64 / *this.total as f64);
        }

        let mut dirty = this.core.tick(cx);

        while let Poll::Ready(Some(msg)) = this.messages.as_mut().poll_next(cx) {
            this.core.set_message(msg.into());
            dirty = true;
        }

        let mut i = 0;
        while i < this.futs.len() {
            match this.futs[i].as_mut().poll(cx) {
                Poll::Ready(out) => {
                    this.results.push(out);
                    drop(this.futs.swap_remove(i));
                    *this.completed += 1;
                    if *this.total > 0 {
                        this.core
                            .state
                            .set_progress(*this.completed as f64 / *this.total as f64);
                    }
                    dirty = true;
                }
                Poll::Pending => i += 1,
            }
        }

        if !this.futs.is_empty() && dirty {
            this.core.render();
        }

        if this.futs.is_empty() {
            this.core.clear();
            Poll::Ready(std::mem::take(this.results))
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_io::Timer;
    use futures_lite::future;

    use super::*;

    #[test]
    fn empty_input_resolves_immediately() {
        future::block_on(async {
            let results: Vec<()> = join(Vec::<futures_lite::future::Ready<()>>::new()).await;
            assert!(results.is_empty());
        });
    }

    #[test]
    fn returns_results_in_completion_order() {
        future::block_on(async {
            let futs = [(60, "slow"), (20, "fast"), (40, "medium")]
                .into_iter()
                .map(|(ms, name)| async move {
                    Timer::after(Duration::from_millis(ms)).await;
                    name
                });
            let results = join(futs).await;
            assert_eq!(results, vec!["fast", "medium", "slow"]);
        });
    }

    #[test]
    fn progress_reflects_completion_fraction() {
        let j: Join<futures_lite::future::Ready<()>> = Join::new(Vec::new());
        assert!(j.progress().is_none());

        let j = join(vec![futures_lite::future::ready(()); 4]);
        assert_eq!(j.progress(), Some(0.0));
    }
}
