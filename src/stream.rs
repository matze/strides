//! Progress integration for streams.
//!
//! Four entry points on [`StreamExt`]:
//!
//! - [`progress`](StreamExt::progress) takes a fraction closure; appropriate when each item
//!   already carries enough information to compute completion in `0.0..=1.0`. Sugar for
//!   `self.progressive(fraction_fn).with_theme(theme)`.
//! - [`progress_count`](StreamExt::progress_count) counts items internally; pair with
//!   [`with_len`](ProgressCountStream::with_len) to derive the bar fraction from `count / total`.
//!   The closure-free path for "I know how many items will flow through".
//! - [`progress_bytes`](StreamExt::progress_bytes) takes a byte-delta closure. The builder owns
//!   the cumulative counter, EWMA rate and (when
//!   [`with_len`](ProgressBytesStream::with_len) is set) the derived progress fraction. Pair with
//!   [`Segment::bytes`](crate::layout::Segment::bytes),
//!   [`Segment::rate`](crate::layout::Segment::rate) and [`Segment::eta`](crate::layout::Segment::eta)
//!   in a custom [`Layout`](crate::layout::Layout) for byte / throughput / ETA columns.
//! - [`progressive`](StreamExt::progressive), [`progressive_count`](StreamExt::progressive_count)
//!   and [`progressive_bytes`](StreamExt::progressive_bytes) produce unconfigured adapters. Without
//!   [`with_theme`](ProgressStream::with_theme) they inherit the parent [`Group`]'s theme; with
//!   `with_theme` they render standalone or override the Group's theme per-row.
//!
//! Dynamic messages compose on top of any builder via
//! [`with_messages`](ProgressStream::with_messages).

pub mod group;

pub use group::Group;

use std::borrow::Cow;
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
    /// A [`Stream`] wrapped to track progress derived from a fraction closure.
    pub struct ProgressStream<S, F, M = Pending<&'static str>> {
        #[pin]
        inner: S,
        fraction_fn: F,
        #[pin]
        messages: M,
        core: Progress,
        current: usize,
    }
}

impl<S, F> ProgressStream<S, F> {
    fn new(inner: S, fraction_fn: F) -> Self {
        Self {
            inner,
            fraction_fn,
            messages: stream::pending(),
            core: Progress::new(),
            current: 0,
        }
    }
}

impl<S, F, M> ProgressStream<S, F, M> {
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

    /// Render this row with `theme`. Drives standalone rendering when the stream is polled
    /// directly; overrides the parent [`Group`]'s theme when pushed.
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

    /// Replace the displayed message each time `messages` yields a value. The item type is
    /// anything that converts into a `Cow<'static, str>`: `&'static str` and `String` are
    /// zero-copy; other formatted values should be `format!`'d at the call site.
    pub fn with_messages<S2>(self, messages: S2) -> ProgressStream<S, F, S2>
    where
        S2: Stream,
        S2::Item: Into<Cow<'static, str>>,
    {
        ProgressStream {
            inner: self.inner,
            fraction_fn: self.fraction_fn,
            messages,
            core: self.core,
            current: self.current,
        }
    }
}

impl<S, F, M> Progressive for ProgressStream<S, F, M> {
    fn label(&self) -> Option<&str> {
        self.core.label()
    }
    fn message(&self) -> Option<&str> {
        self.core.message()
    }
    fn progress(&self) -> Option<f64> {
        self.core.progress()
    }
    fn bytes_done(&self) -> u64 {
        self.core.bytes_done()
    }
    fn bytes_total(&self) -> Option<u64> {
        self.core.bytes_total()
    }
    fn rate(&self) -> Option<f64> {
        self.core.rate()
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

impl<S, F, M> Stream for ProgressStream<S, F, M>
where
    S: Stream,
    F: FnMut(usize, &S::Item) -> f64,
    M: Stream,
    M::Item: Into<Cow<'static, str>>,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        this.core.materialize();

        let mut dirty = this.core.tick(cx);

        while let Poll::Ready(Some(msg)) = this.messages.as_mut().poll_next(cx) {
            this.core.set_message(msg.into());
            dirty = true;
        }

        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(item)) => {
                let completed = (this.fraction_fn)(*this.current, &item);
                *this.current += 1;
                this.core.state.set_progress(completed);
                this.core.render();
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                this.core.clear();
                Poll::Ready(None)
            }
            Poll::Pending => {
                // Keep the spinner animating while the inner stream stalls: the tick's timer
                // wakes this task even when no items flow.
                if dirty {
                    this.core.render();
                }

                Poll::Pending
            }
        }
    }
}

/// Extension trait that adds progress display to streams.
pub trait StreamExt: Stream {
    /// Wrap this stream as a [`ProgressStream`] configured for standalone rendering with `theme`
    /// and a fraction closure. Sugar for `self.progressive(fraction_fn).with_theme(theme)`. The
    /// closure receives the zero-based item index and a reference to the item, matching
    /// [`Iterator::enumerate`].
    fn progress<F>(self, theme: impl Into<Theme>, fraction_fn: F) -> ProgressStream<Self, F>
    where
        Self: Sized,
        F: FnMut(usize, &Self::Item) -> f64,
    {
        self.progressive(fraction_fn).with_theme(theme)
    }

    /// Wrap this stream as an unconfigured [`ProgressStream`]. Awaited directly it renders with
    /// [`Theme::default()`]; chain [`with_theme`](ProgressStream::with_theme) for a custom theme,
    /// or push into a [`Group`] to inherit the Group's theme.
    fn progressive<F>(self, fraction_fn: F) -> ProgressStream<Self, F>
    where
        Self: Sized,
        F: FnMut(usize, &Self::Item) -> f64,
    {
        ProgressStream::new(self, fraction_fn)
    }

    /// Wrap this stream as a [`ProgressBytesStream`] configured for standalone rendering with
    /// `theme` and a byte-delta closure. Sugar for
    /// `self.progressive_bytes(bytes_fn).with_theme(theme)`.
    fn progress_bytes<F>(self, theme: impl Into<Theme>, bytes_fn: F) -> ProgressBytesStream<Self, F>
    where
        Self: Sized,
        F: FnMut(&Self::Item) -> u64,
    {
        self.progressive_bytes(bytes_fn).with_theme(theme)
    }

    /// Wrap this stream as an unconfigured [`ProgressBytesStream`]. Same theme-inheritance rules
    /// as [`progressive`](Self::progressive).
    fn progressive_bytes<F>(self, bytes_fn: F) -> ProgressBytesStream<Self, F>
    where
        Self: Sized,
        F: FnMut(&Self::Item) -> u64,
    {
        ProgressBytesStream::new(self, bytes_fn)
    }

    /// Wrap this stream as a [`ProgressCountStream`] configured for standalone rendering with
    /// `theme`. Items are counted internally; chain
    /// [`with_len`](ProgressCountStream::with_len) to enable the bar. Sugar for
    /// `self.progressive_count().with_theme(theme)`.
    fn progress_count(self, theme: impl Into<Theme>) -> ProgressCountStream<Self>
    where
        Self: Sized,
    {
        self.progressive_count().with_theme(theme)
    }

    /// Wrap this stream as an unconfigured [`ProgressCountStream`]. Same theme-inheritance rules
    /// as [`progressive`](Self::progressive).
    fn progressive_count(self) -> ProgressCountStream<Self>
    where
        Self: Sized,
    {
        ProgressCountStream::new(self)
    }
}

impl<S> StreamExt for S where S: Stream {}

pin_project! {
    /// A [`Stream`] wrapped to track cumulative bytes, smoothed rate and (optionally) total.
    pub struct ProgressBytesStream<S, F, M = Pending<&'static str>> {
        #[pin]
        inner: S,
        bytes_fn: F,
        #[pin]
        messages: M,
        core: Progress,
    }
}

impl<S, F> ProgressBytesStream<S, F> {
    fn new(inner: S, bytes_fn: F) -> Self {
        Self {
            inner,
            bytes_fn,
            messages: stream::pending(),
            core: Progress::new(),
        }
    }
}

impl<S, F, M> ProgressBytesStream<S, F, M> {
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

    /// Record the total number of bytes expected. Enables the bar and the ETA segment.
    pub fn with_len(mut self, total: u64) -> Self {
        self.core.state.set_bytes_total(total);
        self
    }

    /// Render this row with `theme`. Drives standalone rendering when the stream is polled
    /// directly; overrides the parent [`Group`]'s theme when pushed.
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

    /// Replace the displayed message each time `messages` yields a value. The item type is
    /// anything that converts into a `Cow<'static, str>`: `&'static str` and `String` are
    /// zero-copy; other formatted values should be `format!`'d at the call site.
    pub fn with_messages<S2>(self, messages: S2) -> ProgressBytesStream<S, F, S2>
    where
        S2: Stream,
        S2::Item: Into<Cow<'static, str>>,
    {
        ProgressBytesStream {
            inner: self.inner,
            bytes_fn: self.bytes_fn,
            messages,
            core: self.core,
        }
    }
}

impl<S, F, M> Progressive for ProgressBytesStream<S, F, M> {
    fn label(&self) -> Option<&str> {
        self.core.label()
    }
    fn message(&self) -> Option<&str> {
        self.core.message()
    }
    fn progress(&self) -> Option<f64> {
        self.core.progress()
    }
    fn bytes_done(&self) -> u64 {
        self.core.bytes_done()
    }
    fn bytes_total(&self) -> Option<u64> {
        self.core.bytes_total()
    }
    fn rate(&self) -> Option<f64> {
        self.core.rate()
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

impl<S, F, M> Stream for ProgressBytesStream<S, F, M>
where
    S: Stream,
    F: FnMut(&S::Item) -> u64,
    M: Stream,
    M::Item: Into<Cow<'static, str>>,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        this.core.materialize();

        let mut dirty = this.core.tick(cx);

        while let Poll::Ready(Some(msg)) = this.messages.as_mut().poll_next(cx) {
            this.core.set_message(msg.into());
            dirty = true;
        }

        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(item)) => {
                let delta = (this.bytes_fn)(&item);
                this.core.state.add_bytes(delta);
                this.core.render();
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                this.core.clear();
                Poll::Ready(None)
            }
            Poll::Pending => {
                // Keep the spinner animating while the inner stream stalls: the tick's timer
                // wakes this task even when no items flow.
                if dirty {
                    this.core.render();
                }

                Poll::Pending
            }
        }
    }
}

pin_project! {
    /// A [`Stream`] wrapped to count items and derive a progress fraction from a known total.
    ///
    /// The total is seeded from [`Stream::size_hint`]'s upper bound at construction, so bounded
    /// sources like `iter(Vec)` and `iter(0..n)` render a filled bar with no extra ceremony.
    /// Streams whose hint upper-bound is `None` (channels, `pending`, most combinators that can
    /// shorten or extend) render only the spinner, label, message and elapsed-time segments.
    /// [`with_len`](Self::with_len) overrides the hint when the caller knows better.
    pub struct ProgressCountStream<S, M = Pending<&'static str>> {
        #[pin]
        inner: S,
        #[pin]
        messages: M,
        core: Progress,
        current: u64,
        total: Option<u64>,
    }
}

impl<S: Stream> ProgressCountStream<S> {
    fn new(inner: S) -> Self {
        // Best-effort total from the stream's size hint. Exact for bounded sources like
        // `iter(Vec)` or `iter(0..n)`; combinators like `.filter()` lose accuracy but their
        // upper-bound stays a safe over-estimate. Explicit [`with_len`](Self::with_len) wins.
        let total = inner.size_hint().1.map(|n| n as u64);
        Self {
            inner,
            messages: stream::pending(),
            core: Progress::new(),
            current: 0,
            total,
        }
    }
}

impl<S, M> ProgressCountStream<S, M> {
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

    /// Record the total number of items expected, overriding any total derived from
    /// [`Stream::size_hint`]. Enables the bar when the size hint did not.
    pub fn with_len(mut self, total: u64) -> Self {
        self.total = Some(total);
        self
    }

    /// Render this row with `theme`. Drives standalone rendering when the stream is polled
    /// directly; overrides the parent [`Group`]'s theme when pushed.
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

    /// Replace the displayed message each time `messages` yields a value. The item type is
    /// anything that converts into a `Cow<'static, str>`: `&'static str` and `String` are
    /// zero-copy; other formatted values should be `format!`'d at the call site.
    pub fn with_messages<S2>(self, messages: S2) -> ProgressCountStream<S, S2>
    where
        S2: Stream,
        S2::Item: Into<Cow<'static, str>>,
    {
        ProgressCountStream {
            inner: self.inner,
            messages,
            core: self.core,
            current: self.current,
            total: self.total,
        }
    }
}

impl<S, M> Progressive for ProgressCountStream<S, M> {
    fn label(&self) -> Option<&str> {
        self.core.label()
    }
    fn message(&self) -> Option<&str> {
        self.core.message()
    }
    fn progress(&self) -> Option<f64> {
        self.core.progress()
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

impl<S, M> Stream for ProgressCountStream<S, M>
where
    S: Stream,
    M: Stream,
    M::Item: Into<Cow<'static, str>>,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        this.core.materialize();

        let mut dirty = this.core.tick(cx);

        while let Poll::Ready(Some(msg)) = this.messages.as_mut().poll_next(cx) {
            this.core.set_message(msg.into());
            dirty = true;
        }

        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(item)) => {
                *this.current += 1;
                if let Some(total) = *this.total {
                    if total > 0 {
                        this.core
                            .state
                            .set_progress(*this.current as f64 / total as f64);
                    }
                }
                this.core.render();
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                this.core.clear();
                Poll::Ready(None)
            }
            Poll::Pending => {
                // Keep the spinner animating while the inner stream stalls: the tick's timer
                // wakes this task even when no items flow.
                if dirty {
                    this.core.render();
                }

                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use futures_lite::{future, stream, StreamExt as FlStreamExt};

    #[test]
    fn progress_closure_sees_zero_based_index() {
        future::block_on(async {
            let seen = Arc::new(Mutex::new(Vec::new()));
            let recorded = seen.clone();
            let s = stream::iter(['a', 'b', 'c']).progressive(move |i, _| {
                recorded.lock().unwrap().push(i);
                0.0
            });
            let mut s = Box::pin(s);
            while s.next().await.is_some() {}
            assert_eq!(*seen.lock().unwrap(), vec![0, 1, 2]);
        });
    }

    #[test]
    fn count_seeds_total_from_size_hint() {
        future::block_on(async {
            let s = stream::iter(0..4u32).progressive_count();
            let mut s = Box::pin(s);
            assert_eq!(Progressive::progress(&*s), None);
            for expected in [0.25, 0.5, 0.75, 1.0] {
                s.next().await.unwrap();
                assert_eq!(Progressive::progress(&*s), Some(expected));
            }
            assert!(s.next().await.is_none());
        });
    }

    #[test]
    fn count_without_size_hint_keeps_progress_absent() {
        // `stream::pending::<u32>()` has size_hint `(0, None)` and yields nothing.
        let s: ProgressCountStream<_> = stream::pending::<u32>().progressive_count();
        assert_eq!(Progressive::progress(&s), None);
    }

    #[test]
    fn count_with_len_overrides_size_hint() {
        future::block_on(async {
            // size_hint says 4; user knows the real working set is 2.
            let s = stream::iter(0..4u32).progressive_count().with_len(2);
            let mut s = Box::pin(s);
            s.next().await.unwrap();
            assert_eq!(Progressive::progress(&*s), Some(0.5));
            s.next().await.unwrap();
            assert_eq!(Progressive::progress(&*s), Some(1.0));
        });
    }

    #[test]
    fn count_with_zero_len_does_not_set_progress() {
        future::block_on(async {
            let s = stream::iter(0..2u32).progressive_count().with_len(0);
            let mut s = Box::pin(s);
            while s.next().await.is_some() {}
            assert_eq!(Progressive::progress(&*s), None);
        });
    }
}
