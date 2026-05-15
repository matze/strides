//! Progress integration for streams.
//!
//! Three entry points on [`StreamExt`]:
//!
//! - [`progress`](StreamExt::progress) takes a fraction closure; appropriate when each item
//!   already carries enough information to compute completion in `0.0..=1.0`.
//! - [`progress_bytes`](StreamExt::progress_bytes) takes a byte-delta closure. The builder owns
//!   the cumulative counter, EWMA rate and (when
//!   [`with_len`](ProgressBytesStream::with_len) is set) the derived progress fraction. Pair with
//!   [`Segment::bytes`](crate::layout::Segment::bytes),
//!   [`Segment::rate`](crate::layout::Segment::rate) and [`Segment::eta`](crate::layout::Segment::eta)
//!   in a custom [`Layout`](crate::layout::Layout) for byte / throughput / ETA columns.
//! - [`progressive`](StreamExt::progressive) and [`progressive_bytes`](StreamExt::progressive_bytes)
//!   produce tracked-only adapters for inclusion in a [`Group`] — they don't render on their own.
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

use crate::line::{FrameContext, Line};
use crate::progressive::Progressive;
use crate::spinner::Ticks;
use crate::state::State;
use crate::term::CursorGuard;
use crate::Theme;

/// Standalone rendering state for stream wrappers.
struct Rendering<'a> {
    line: Line<'a>,
    ticks: Ticks<'a>,
    spinner_char: Option<char>,
    spinner_style: Style,
    annotation_style: Style,
    is_tty: bool,
    _guard: CursorGuard,
}

/// A [`Stream`] wrapped to track progress derived from a fraction closure.
pub struct ProgressStream<'a, S, F, M = Pending<&'static str>> {
    inner: S,
    fraction_fn: F,
    messages: M,
    state: State,
    current: usize,
    rendering: Option<Rendering<'a>>,
}

impl<'a, S, F> ProgressStream<'a, S, F> {
    fn new(inner: S, fraction_fn: F) -> Self {
        Self {
            inner,
            fraction_fn,
            messages: stream::pending(),
            state: State::new(),
            current: 0,
            rendering: None,
        }
    }

    fn standalone(inner: S, fraction_fn: F, theme: Theme<'a>) -> Self {
        let is_tty = std::io::stdout().is_terminal();
        let ticks = theme.spinner.ticks();
        let line = Line::new(&theme);
        Self {
            inner,
            fraction_fn,
            messages: stream::pending(),
            state: State::new(),
            current: 0,
            rendering: Some(Rendering {
                line,
                ticks,
                spinner_char: None,
                spinner_style: Style::new(),
                annotation_style: Style::new(),
                is_tty,
                _guard: CursorGuard { is_tty },
            }),
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

    /// Replace the displayed message each time `messages` yields a value.
    pub fn with_messages<S2>(self, messages: S2) -> ProgressStream<'a, S, F, S2>
    where
        S2: Stream + Unpin,
        S2::Item: Display,
    {
        ProgressStream {
            inner: self.inner,
            fraction_fn: self.fraction_fn,
            messages,
            state: self.state,
            current: self.current,
            rendering: self.rendering,
        }
    }
}

impl<S, F, M> Progressive for ProgressStream<'_, S, F, M> {
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
}

impl<S, F, M> Stream for ProgressStream<'_, S, F, M>
where
    S: Stream + Unpin,
    F: FnMut(usize, &S::Item) -> f64 + Unpin,
    M: Stream + Unpin,
    M::Item: Display,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if let Some(r) = this.rendering.as_mut() {
            if let Poll::Ready(ch) = Pin::new(&mut r.ticks).poll_next(cx) {
                r.spinner_char = ch;
            }
        }

        while let Poll::Ready(Some(msg)) = Pin::new(&mut this.messages).poll_next(cx) {
            this.state.set_message(msg.to_string());
        }

        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(item)) => {
                this.current += 1;
                let completed = (this.fraction_fn)(this.current, &item);
                this.state.set_progress(completed);
                if let Some(r) = this.rendering.as_mut() {
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
                    r.line.standalone_render(&this.state, &frame, r.is_tty);
                }
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                if let Some(r) = this.rendering.as_ref() {
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
    /// Wrap this stream as a standalone [`ProgressStream`] driven by `theme` and a fraction
    /// closure. The closure receives the monotonically increasing item index (starting at 1) and
    /// a reference to the item.
    fn progress<'a, F>(
        self,
        theme: impl Into<Theme<'a>>,
        fraction_fn: F,
    ) -> ProgressStream<'a, Self, F>
    where
        Self: Sized,
        F: FnMut(usize, &Self::Item) -> f64 + Unpin,
    {
        ProgressStream::standalone(self, fraction_fn, theme.into())
    }

    /// Wrap this stream as a tracked-only [`ProgressStream`] for [`Group::push`]. Does not render
    /// on its own.
    fn progressive<'a, F>(self, fraction_fn: F) -> ProgressStream<'a, Self, F>
    where
        Self: Sized,
        F: FnMut(usize, &Self::Item) -> f64 + Unpin,
    {
        ProgressStream::new(self, fraction_fn)
    }

    /// Wrap this stream as a standalone [`ProgressBytesStream`] driven by `theme` and a byte-delta
    /// closure. The builder accumulates the cumulative byte counter, EWMA rate and (when a total
    /// is set) the progress fraction.
    fn progress_bytes<'a, F>(
        self,
        theme: impl Into<Theme<'a>>,
        bytes_fn: F,
    ) -> ProgressBytesStream<'a, Self, F>
    where
        Self: Sized,
        F: FnMut(&Self::Item) -> u64 + Unpin,
    {
        ProgressBytesStream::standalone(self, bytes_fn, theme.into())
    }

    /// Wrap this stream as a tracked-only [`ProgressBytesStream`] for [`Group::push`].
    fn progressive_bytes<'a, F>(self, bytes_fn: F) -> ProgressBytesStream<'a, Self, F>
    where
        Self: Sized,
        F: FnMut(&Self::Item) -> u64 + Unpin,
    {
        ProgressBytesStream::new(self, bytes_fn)
    }
}

impl<S> StreamExt for S where S: Stream {}

/// A [`Stream`] wrapped to track cumulative bytes, smoothed rate and (optionally) total.
pub struct ProgressBytesStream<'a, S, F, M = Pending<&'static str>> {
    inner: S,
    bytes_fn: F,
    messages: M,
    state: State,
    rendering: Option<Rendering<'a>>,
}

impl<'a, S, F> ProgressBytesStream<'a, S, F> {
    fn new(inner: S, bytes_fn: F) -> Self {
        Self {
            inner,
            bytes_fn,
            messages: stream::pending(),
            state: State::new(),
            rendering: None,
        }
    }

    fn standalone(inner: S, bytes_fn: F, theme: Theme<'a>) -> Self {
        let is_tty = std::io::stdout().is_terminal();
        let ticks = theme.spinner.ticks();
        let line = Line::new(&theme);
        Self {
            inner,
            bytes_fn,
            messages: stream::pending(),
            state: State::new(),
            rendering: Some(Rendering {
                line,
                ticks,
                spinner_char: None,
                spinner_style: Style::new(),
                annotation_style: Style::new(),
                is_tty,
                _guard: CursorGuard { is_tty },
            }),
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

    /// Replace the displayed message each time `messages` yields a value.
    pub fn with_messages<S2>(self, messages: S2) -> ProgressBytesStream<'a, S, F, S2>
    where
        S2: Stream + Unpin,
        S2::Item: Display,
    {
        ProgressBytesStream {
            inner: self.inner,
            bytes_fn: self.bytes_fn,
            messages,
            state: self.state,
            rendering: self.rendering,
        }
    }
}

impl<S, F, M> Progressive for ProgressBytesStream<'_, S, F, M> {
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
}

impl<S, F, M> Stream for ProgressBytesStream<'_, S, F, M>
where
    S: Stream + Unpin,
    F: FnMut(&S::Item) -> u64 + Unpin,
    M: Stream + Unpin,
    M::Item: Display,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if let Some(r) = this.rendering.as_mut() {
            if let Poll::Ready(ch) = Pin::new(&mut r.ticks).poll_next(cx) {
                r.spinner_char = ch;
            }
        }

        while let Poll::Ready(Some(msg)) = Pin::new(&mut this.messages).poll_next(cx) {
            this.state.set_message(msg.to_string());
        }

        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(item)) => {
                let delta = (this.bytes_fn)(&item);
                this.state.add_bytes(delta);
                if let Some(r) = this.rendering.as_mut() {
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
                    r.line.standalone_render(&this.state, &frame, r.is_tty);
                }
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                if let Some(r) = this.rendering.as_ref() {
                    Line::standalone_clear(r.is_tty);
                }
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
