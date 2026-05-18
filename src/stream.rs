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

use std::fmt::Display;
use std::io::IsTerminal;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_lite::stream::Pending;
use futures_lite::{stream, Stream};
use owo_colors::Style;
use pin_project_lite::pin_project;

use crate::line::{FrameContext, Line};
use crate::progressive::Progressive;
use crate::spinner::Ticks;
use crate::state::State;
use crate::term::CursorGuard;
use crate::Theme;

/// Materialised rendering bits used by the standalone path.
struct Rendering<'a> {
    line: Line<'a>,
    ticks: Ticks<'a>,
    spinner_char: Option<char>,
    spinner_style: Style,
    annotation_style: Style,
    is_tty: bool,
    _guard: CursorGuard,
}

/// Lifecycle of the standalone rendering bits.
enum RenderingState<'a> {
    /// Constructed but not yet polled. Materialise on first poll using the row's theme override
    /// (or [`Theme::default()`] when none was set).
    Pending,
    /// Materialised; standalone rendering is active.
    Active(Rendering<'a>),
    /// A [`Group`] owns rendering for this row; no standalone rendering will happen.
    Detached,
}

pin_project! {
    /// A [`Stream`] wrapped to track progress derived from a fraction closure.
    pub struct ProgressStream<'a, S, F, M = Pending<&'static str>> {
        #[pin]
        inner: S,
        fraction_fn: F,
        #[pin]
        messages: M,
        state: State,
        current: usize,
        theme_override: Option<Theme<'a>>,
        spinner_style_override: Option<Style>,
        annotation_style_override: Option<Style>,
        rendering: RenderingState<'a>,
    }
}

impl<S, F> ProgressStream<'_, S, F> {
    fn new(inner: S, fraction_fn: F) -> Self {
        Self {
            inner,
            fraction_fn,
            messages: stream::pending(),
            state: State::new(),
            current: 0,
            theme_override: None,
            spinner_style_override: None,
            annotation_style_override: None,
            rendering: RenderingState::Pending,
        }
    }
}

impl<'a, S, F, M> ProgressStream<'a, S, F, M> {
    /// Set the static label shown in the [`Label`](crate::layout::Segment::Label) segment.
    pub fn with_label(mut self, label: impl Display) -> Self {
        self.state.set_label(label.to_string());
        self
    }

    /// Prepend the elapsed time to the line.
    pub fn with_elapsed_time(mut self) -> Self {
        self.state.enable_elapsed_time();
        self
    }

    /// Render this row with `theme`. Drives standalone rendering when the stream is polled
    /// directly; overrides the parent [`Group`]'s theme when pushed.
    pub fn with_theme(mut self, theme: impl Into<Theme<'a>>) -> Self {
        self.theme_override = Some(theme.into());
        self
    }

    /// Apply `style` to the spinner character on this row, overriding the parent Group's default.
    pub fn with_spinner_style(mut self, style: Style) -> Self {
        self.spinner_style_override = Some(style);
        self
    }

    /// Apply `style` to the annotation (label) text on this row, overriding the parent Group's
    /// default.
    pub fn with_annotation_style(mut self, style: Style) -> Self {
        self.annotation_style_override = Some(style);
        self
    }

    /// Replace the displayed message each time `messages` yields a value.
    pub fn with_messages<S2>(self, messages: S2) -> ProgressStream<'a, S, F, S2>
    where
        S2: Stream,
        S2::Item: Display,
    {
        ProgressStream {
            inner: self.inner,
            fraction_fn: self.fraction_fn,
            messages,
            state: self.state,
            current: self.current,
            theme_override: self.theme_override,
            spinner_style_override: self.spinner_style_override,
            annotation_style_override: self.annotation_style_override,
            rendering: self.rendering,
        }
    }
}

impl<'a, S, F, M> Progressive<'a> for ProgressStream<'a, S, F, M> {
    fn label(&self) -> Option<&str> {
        self.state.label()
    }
    fn message(&self) -> Option<&str> {
        self.state.message()
    }
    fn progress(&self) -> Option<f64> {
        self.state.progress()
    }
    fn bytes_done(&self) -> u64 {
        self.state.bytes_done()
    }
    fn bytes_total(&self) -> Option<u64> {
        self.state.bytes_total()
    }
    fn rate(&self) -> Option<f64> {
        self.state.rate()
    }
    fn detach_rendering(&mut self) {
        self.rendering = RenderingState::Detached;
    }
    fn theme(&self) -> Option<&Theme<'a>> {
        self.theme_override.as_ref()
    }
    fn spinner_style(&self) -> Option<Style> {
        self.spinner_style_override
    }
    fn annotation_style(&self) -> Option<Style> {
        self.annotation_style_override
    }
    fn show_elapsed_time(&self) -> Option<bool> {
        if self.state.with_elapsed_time {
            Some(true)
        } else {
            None
        }
    }
}

impl<S, F, M> Stream for ProgressStream<'_, S, F, M>
where
    S: Stream,
    F: FnMut(usize, &S::Item) -> f64,
    M: Stream,
    M::Item: Display,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        materialize_rendering(
            this.rendering,
            this.theme_override.as_ref(),
            *this.spinner_style_override,
            *this.annotation_style_override,
        );

        if let RenderingState::Active(r) = &mut *this.rendering {
            if let Poll::Ready(ch) = Pin::new(&mut r.ticks).poll_next(cx) {
                r.spinner_char = ch;
            }
        }

        while let Poll::Ready(Some(msg)) = this.messages.as_mut().poll_next(cx) {
            this.state.set_message(msg.to_string());
        }

        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(item)) => {
                *this.current += 1;
                let completed = (this.fraction_fn)(*this.current, &item);
                this.state.set_progress(completed);
                if let RenderingState::Active(r) = &mut *this.rendering {
                    let elapsed = if this.state.with_elapsed_time {
                        this.state.elapsed()
                    } else {
                        Duration::ZERO
                    };
                    let frame = FrameContext {
                        spinner_char: r.spinner_char,
                        elapsed,
                        show_elapsed: this.state.with_elapsed_time,
                        spinner_style: r.spinner_style,
                        annotation_style: r.annotation_style,
                    };
                    r.line.standalone_render(this.state, &frame, r.is_tty);
                }
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                if let RenderingState::Active(r) = &*this.rendering {
                    Line::standalone_clear(r.is_tty);
                }
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Extension trait that adds progress display to streams.
pub trait StreamExt: Stream {
    /// Wrap this stream as a [`ProgressStream`] configured for standalone rendering with `theme`
    /// and a fraction closure. Sugar for `self.progressive(fraction_fn).with_theme(theme)`. The
    /// closure receives the monotonically increasing item index (starting at 1) and a reference
    /// to the item.
    fn progress<'a, F>(
        self,
        theme: impl Into<Theme<'a>>,
        fraction_fn: F,
    ) -> ProgressStream<'a, Self, F>
    where
        Self: Sized,
        F: FnMut(usize, &Self::Item) -> f64,
    {
        self.progressive(fraction_fn).with_theme(theme)
    }

    /// Wrap this stream as an unconfigured [`ProgressStream`]. Awaited directly it renders with
    /// [`Theme::default()`]; chain [`with_theme`](ProgressStream::with_theme) for a custom theme,
    /// or push into a [`Group`] to inherit the Group's theme.
    fn progressive<'a, F>(self, fraction_fn: F) -> ProgressStream<'a, Self, F>
    where
        Self: Sized,
        F: FnMut(usize, &Self::Item) -> f64,
    {
        ProgressStream::new(self, fraction_fn)
    }

    /// Wrap this stream as a [`ProgressBytesStream`] configured for standalone rendering with
    /// `theme` and a byte-delta closure. Sugar for
    /// `self.progressive_bytes(bytes_fn).with_theme(theme)`.
    fn progress_bytes<'a, F>(
        self,
        theme: impl Into<Theme<'a>>,
        bytes_fn: F,
    ) -> ProgressBytesStream<'a, Self, F>
    where
        Self: Sized,
        F: FnMut(&Self::Item) -> u64,
    {
        self.progressive_bytes(bytes_fn).with_theme(theme)
    }

    /// Wrap this stream as an unconfigured [`ProgressBytesStream`]. Same theme-inheritance rules
    /// as [`progressive`](Self::progressive).
    fn progressive_bytes<'a, F>(self, bytes_fn: F) -> ProgressBytesStream<'a, Self, F>
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
    fn progress_count<'a>(self, theme: impl Into<Theme<'a>>) -> ProgressCountStream<'a, Self>
    where
        Self: Sized,
    {
        self.progressive_count().with_theme(theme)
    }

    /// Wrap this stream as an unconfigured [`ProgressCountStream`]. Same theme-inheritance rules
    /// as [`progressive`](Self::progressive).
    fn progressive_count<'a>(self) -> ProgressCountStream<'a, Self>
    where
        Self: Sized,
    {
        ProgressCountStream::new(self)
    }
}

impl<S> StreamExt for S where S: Stream {}

pin_project! {
    /// A [`Stream`] wrapped to track cumulative bytes, smoothed rate and (optionally) total.
    pub struct ProgressBytesStream<'a, S, F, M = Pending<&'static str>> {
        #[pin]
        inner: S,
        bytes_fn: F,
        #[pin]
        messages: M,
        state: State,
        theme_override: Option<Theme<'a>>,
        spinner_style_override: Option<Style>,
        annotation_style_override: Option<Style>,
        rendering: RenderingState<'a>,
    }
}

impl<S, F> ProgressBytesStream<'_, S, F> {
    fn new(inner: S, bytes_fn: F) -> Self {
        Self {
            inner,
            bytes_fn,
            messages: stream::pending(),
            state: State::new(),
            theme_override: None,
            spinner_style_override: None,
            annotation_style_override: None,
            rendering: RenderingState::Pending,
        }
    }
}

impl<'a, S, F, M> ProgressBytesStream<'a, S, F, M> {
    /// Set the static label shown in the [`Label`](crate::layout::Segment::Label) segment.
    pub fn with_label(mut self, label: impl Display) -> Self {
        self.state.set_label(label.to_string());
        self
    }

    /// Prepend the elapsed time to the line.
    pub fn with_elapsed_time(mut self) -> Self {
        self.state.enable_elapsed_time();
        self
    }

    /// Record the total number of bytes expected. Enables the bar and the ETA segment.
    pub fn with_len(mut self, total: u64) -> Self {
        self.state.set_bytes_total(total);
        self
    }

    /// Render this row with `theme`. Drives standalone rendering when the stream is polled
    /// directly; overrides the parent [`Group`]'s theme when pushed.
    pub fn with_theme(mut self, theme: impl Into<Theme<'a>>) -> Self {
        self.theme_override = Some(theme.into());
        self
    }

    /// Apply `style` to the spinner character on this row, overriding the parent Group's default.
    pub fn with_spinner_style(mut self, style: Style) -> Self {
        self.spinner_style_override = Some(style);
        self
    }

    /// Apply `style` to the annotation (label) text on this row, overriding the parent Group's
    /// default.
    pub fn with_annotation_style(mut self, style: Style) -> Self {
        self.annotation_style_override = Some(style);
        self
    }

    /// Replace the displayed message each time `messages` yields a value.
    pub fn with_messages<S2>(self, messages: S2) -> ProgressBytesStream<'a, S, F, S2>
    where
        S2: Stream,
        S2::Item: Display,
    {
        ProgressBytesStream {
            inner: self.inner,
            bytes_fn: self.bytes_fn,
            messages,
            state: self.state,
            theme_override: self.theme_override,
            spinner_style_override: self.spinner_style_override,
            annotation_style_override: self.annotation_style_override,
            rendering: self.rendering,
        }
    }
}

impl<'a, S, F, M> Progressive<'a> for ProgressBytesStream<'a, S, F, M> {
    fn label(&self) -> Option<&str> {
        self.state.label()
    }
    fn message(&self) -> Option<&str> {
        self.state.message()
    }
    fn progress(&self) -> Option<f64> {
        self.state.progress()
    }
    fn bytes_done(&self) -> u64 {
        self.state.bytes_done()
    }
    fn bytes_total(&self) -> Option<u64> {
        self.state.bytes_total()
    }
    fn rate(&self) -> Option<f64> {
        self.state.rate()
    }
    fn detach_rendering(&mut self) {
        self.rendering = RenderingState::Detached;
    }
    fn theme(&self) -> Option<&Theme<'a>> {
        self.theme_override.as_ref()
    }
    fn spinner_style(&self) -> Option<Style> {
        self.spinner_style_override
    }
    fn annotation_style(&self) -> Option<Style> {
        self.annotation_style_override
    }
    fn show_elapsed_time(&self) -> Option<bool> {
        if self.state.with_elapsed_time {
            Some(true)
        } else {
            None
        }
    }
}

impl<S, F, M> Stream for ProgressBytesStream<'_, S, F, M>
where
    S: Stream,
    F: FnMut(&S::Item) -> u64,
    M: Stream,
    M::Item: Display,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        materialize_rendering(
            this.rendering,
            this.theme_override.as_ref(),
            *this.spinner_style_override,
            *this.annotation_style_override,
        );

        if let RenderingState::Active(r) = &mut *this.rendering {
            if let Poll::Ready(ch) = Pin::new(&mut r.ticks).poll_next(cx) {
                r.spinner_char = ch;
            }
        }

        while let Poll::Ready(Some(msg)) = this.messages.as_mut().poll_next(cx) {
            this.state.set_message(msg.to_string());
        }

        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(item)) => {
                let delta = (this.bytes_fn)(&item);
                this.state.add_bytes(delta);
                if let RenderingState::Active(r) = &mut *this.rendering {
                    let elapsed = if this.state.with_elapsed_time {
                        this.state.elapsed()
                    } else {
                        Duration::ZERO
                    };
                    let frame = FrameContext {
                        spinner_char: r.spinner_char,
                        elapsed,
                        show_elapsed: this.state.with_elapsed_time,
                        spinner_style: r.spinner_style,
                        annotation_style: r.annotation_style,
                    };
                    r.line.standalone_render(this.state, &frame, r.is_tty);
                }
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                if let RenderingState::Active(r) = &*this.rendering {
                    Line::standalone_clear(r.is_tty);
                }
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
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
    pub struct ProgressCountStream<'a, S, M = Pending<&'static str>> {
        #[pin]
        inner: S,
        #[pin]
        messages: M,
        state: State,
        current: u64,
        total: Option<u64>,
        theme_override: Option<Theme<'a>>,
        spinner_style_override: Option<Style>,
        annotation_style_override: Option<Style>,
        rendering: RenderingState<'a>,
    }
}

impl<S: Stream> ProgressCountStream<'_, S> {
    fn new(inner: S) -> Self {
        // Best-effort total from the stream's size hint. Exact for bounded sources like
        // `iter(Vec)` or `iter(0..n)`; combinators like `.filter()` lose accuracy but their
        // upper-bound stays a safe over-estimate. Explicit [`with_len`](Self::with_len) wins.
        let total = inner.size_hint().1.map(|n| n as u64);
        Self {
            inner,
            messages: stream::pending(),
            state: State::new(),
            current: 0,
            total,
            theme_override: None,
            spinner_style_override: None,
            annotation_style_override: None,
            rendering: RenderingState::Pending,
        }
    }
}

impl<'a, S, M> ProgressCountStream<'a, S, M> {
    /// Set the static label shown in the [`Label`](crate::layout::Segment::Label) segment.
    pub fn with_label(mut self, label: impl Display) -> Self {
        self.state.set_label(label.to_string());
        self
    }

    /// Prepend the elapsed time to the line.
    pub fn with_elapsed_time(mut self) -> Self {
        self.state.enable_elapsed_time();
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
    pub fn with_theme(mut self, theme: impl Into<Theme<'a>>) -> Self {
        self.theme_override = Some(theme.into());
        self
    }

    /// Apply `style` to the spinner character on this row, overriding the parent Group's default.
    pub fn with_spinner_style(mut self, style: Style) -> Self {
        self.spinner_style_override = Some(style);
        self
    }

    /// Apply `style` to the annotation (label) text on this row, overriding the parent Group's
    /// default.
    pub fn with_annotation_style(mut self, style: Style) -> Self {
        self.annotation_style_override = Some(style);
        self
    }

    /// Replace the displayed message each time `messages` yields a value.
    pub fn with_messages<S2>(self, messages: S2) -> ProgressCountStream<'a, S, S2>
    where
        S2: Stream,
        S2::Item: Display,
    {
        ProgressCountStream {
            inner: self.inner,
            messages,
            state: self.state,
            current: self.current,
            total: self.total,
            theme_override: self.theme_override,
            spinner_style_override: self.spinner_style_override,
            annotation_style_override: self.annotation_style_override,
            rendering: self.rendering,
        }
    }
}

impl<'a, S, M> Progressive<'a> for ProgressCountStream<'a, S, M> {
    fn label(&self) -> Option<&str> {
        self.state.label()
    }
    fn message(&self) -> Option<&str> {
        self.state.message()
    }
    fn progress(&self) -> Option<f64> {
        self.state.progress()
    }
    fn detach_rendering(&mut self) {
        self.rendering = RenderingState::Detached;
    }
    fn theme(&self) -> Option<&Theme<'a>> {
        self.theme_override.as_ref()
    }
    fn spinner_style(&self) -> Option<Style> {
        self.spinner_style_override
    }
    fn annotation_style(&self) -> Option<Style> {
        self.annotation_style_override
    }
    fn show_elapsed_time(&self) -> Option<bool> {
        if self.state.with_elapsed_time {
            Some(true)
        } else {
            None
        }
    }
}

impl<S, M> Stream for ProgressCountStream<'_, S, M>
where
    S: Stream,
    M: Stream,
    M::Item: Display,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        materialize_rendering(
            this.rendering,
            this.theme_override.as_ref(),
            *this.spinner_style_override,
            *this.annotation_style_override,
        );

        if let RenderingState::Active(r) = &mut *this.rendering {
            if let Poll::Ready(ch) = Pin::new(&mut r.ticks).poll_next(cx) {
                r.spinner_char = ch;
            }
        }

        while let Poll::Ready(Some(msg)) = this.messages.as_mut().poll_next(cx) {
            this.state.set_message(msg.to_string());
        }

        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(item)) => {
                *this.current += 1;
                if let Some(total) = *this.total {
                    if total > 0 {
                        this.state.set_progress(*this.current as f64 / total as f64);
                    }
                }
                if let RenderingState::Active(r) = &mut *this.rendering {
                    let elapsed = if this.state.with_elapsed_time {
                        this.state.elapsed()
                    } else {
                        Duration::ZERO
                    };
                    let frame = FrameContext {
                        spinner_char: r.spinner_char,
                        elapsed,
                        show_elapsed: this.state.with_elapsed_time,
                        spinner_style: r.spinner_style,
                        annotation_style: r.annotation_style,
                    };
                    r.line.standalone_render(this.state, &frame, r.is_tty);
                }
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                if let RenderingState::Active(r) = &*this.rendering {
                    Line::standalone_clear(r.is_tty);
                }
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

fn materialize_rendering<'a>(
    rendering: &mut RenderingState<'a>,
    theme_override: Option<&Theme<'a>>,
    spinner_style_override: Option<Style>,
    annotation_style_override: Option<Style>,
) {
    if !matches!(rendering, RenderingState::Pending) {
        return;
    }
    let theme = theme_override.cloned().unwrap_or_default();
    let is_tty = std::io::stdout().is_terminal();
    let ticks = theme.spinner.ticks();
    let line = Line::new(&theme);
    *rendering = RenderingState::Active(Rendering {
        line,
        ticks,
        spinner_char: None,
        spinner_style: spinner_style_override.unwrap_or_default(),
        annotation_style: annotation_style_override.unwrap_or_default(),
        is_tty,
        _guard: CursorGuard { is_tty },
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_lite::{future, stream, StreamExt as FlStreamExt};

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
        let s: ProgressCountStream<'_, _> = stream::pending::<u32>().progressive_count();
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
