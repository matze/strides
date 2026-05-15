//! Spinner integration for futures.
//!
//! Import [`FutureExt`] to wrap any [`Future`] with progress display. Two entry points:
//!
//! - [`progress(theme)`](FutureExt::progress) returns a [`ProgressFuture`] configured for
//!   standalone use: awaiting it drives a spinner, optional bar and message on its own terminal
//!   line and resolves to the wrapped future's output.
//! - [`progressive()`](FutureExt::progressive) returns a [`ProgressFuture`] configured for
//!   inclusion in a [`Group`]: it does not render on its own; the Group renders all its tasks
//!   together. Configure it with [`with_label`](ProgressFuture::with_label),
//!   [`with_messages`](ProgressFuture::with_messages) and
//!   [`with_progress`](ProgressFuture::with_progress) before pushing.

pub mod group;

pub use group::Group;

use std::fmt::Display;
use std::future::Future;
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

/// Standalone-only fields: own line, spinner ticks, cursor guard.
struct Rendering<'a> {
    line: Line<'a>,
    ticks: Ticks<'a>,
    spinner_char: Option<char>,
    spinner_style: Style,
    annotation_style: Style,
    is_tty: bool,
    _guard: CursorGuard,
}

/// A [`Future`] wrapped with progress state.
///
/// `ProgressFuture` carries the wrapped future together with state read out via [`Progressive`]
/// and (optionally) the rendering machinery for standalone use. The `M` and `P` parameters track
/// the optional message and progress stream types and default to [`Pending`] (a ZST that never
/// yields) so the bare `fut.progress(theme).await` path allocates nothing beyond the spinner
/// [`Ticks`] state.
pub struct ProgressFuture<'a, F, M = Pending<&'static str>, P = Pending<f64>> {
    inner: F,
    messages: M,
    progress: P,
    state: State,
    rendering: Option<Rendering<'a>>,
}

impl<'a, F> ProgressFuture<'a, F> {
    /// Construct a tracked-only `ProgressFuture` with no rendering of its own. Intended for
    /// [`Group::push`].
    pub fn new(inner: F) -> Self {
        Self {
            inner,
            messages: stream::pending(),
            progress: stream::pending(),
            state: State::new(),
            rendering: None,
        }
    }

    /// Construct a standalone `ProgressFuture` that renders to its own terminal row using
    /// `theme`. Driven by `.await`.
    pub fn standalone(inner: F, theme: impl Into<Theme<'a>>) -> Self {
        let theme = theme.into();
        let is_tty = std::io::stdout().is_terminal();
        let ticks = theme.spinner.ticks();
        let line = Line::new(&theme);
        let mut state = State::new();
        // Preserve the legacy behaviour where the bar appears at 0% even with no progress stream.
        state.set_progress(0.0);
        Self {
            inner,
            messages: stream::pending(),
            progress: stream::pending(),
            state,
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

impl<'a, F, M, P> ProgressFuture<'a, F, M, P> {
    /// Set the static label shown in the [`Label`](crate::layout::Segment::Label) segment.
    pub fn with_label(mut self, label: impl Display) -> Self {
        self.state.set_label(label.to_string());
        self
    }

    /// Replace the displayed message each time `messages` yields a value. When the stream is
    /// exhausted the last value remains visible.
    pub fn with_messages<S>(self, messages: S) -> ProgressFuture<'a, F, S, P>
    where
        S: Stream + Unpin,
        S::Item: Display,
    {
        ProgressFuture {
            inner: self.inner,
            messages,
            progress: self.progress,
            state: self.state,
            rendering: self.rendering,
        }
    }

    /// Prepend the elapsed time (seconds since the future was first polled) to the line.
    pub fn with_elapsed_time(mut self) -> Self {
        self.state.enable_elapsed_time();
        self
    }

    /// Drive the progress bar from a stream of fractions in `0.0..=1.0`. The latest value wins.
    pub fn with_progress<S>(self, progress: S) -> ProgressFuture<'a, F, M, S>
    where
        S: Stream<Item = f64> + Unpin,
    {
        ProgressFuture {
            inner: self.inner,
            messages: self.messages,
            progress,
            state: self.state,
            rendering: self.rendering,
        }
    }
}

impl<F, M, P> Progressive for ProgressFuture<'_, F, M, P> {
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

impl<F, M, P> Future for ProgressFuture<'_, F, M, P>
where
    F: Future + Unpin,
    M: Stream + Unpin,
    M::Item: Display,
    P: Stream<Item = f64> + Unpin,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut dirty = false;

        if let Some(r) = this.rendering.as_mut() {
            if let Poll::Ready(ch) = Pin::new(&mut r.ticks).poll_next(cx) {
                r.spinner_char = ch;
                dirty = true;
            }
        }

        while let Poll::Ready(Some(msg)) = Pin::new(&mut this.messages).poll_next(cx) {
            this.state.set_message(msg.to_string());
            dirty = true;
        }

        while let Poll::Ready(Some(p)) = Pin::new(&mut this.progress).poll_next(cx) {
            this.state.set_progress(p.clamp(0.0, 1.0));
            dirty = true;
        }

        let item = Pin::new(&mut this.inner).poll(cx);

        if let Some(r) = this.rendering.as_mut() {
            match item {
                Poll::Pending if dirty => {
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
                Poll::Ready(_) => Line::standalone_clear(r.is_tty),
                _ => {}
            }
        }

        item
    }
}

/// Extension trait that adds progress display to futures.
pub trait FutureExt: Future {
    /// Wrap this future in a standalone [`ProgressFuture`] driven by `theme`. Awaiting the result
    /// renders a spinner, optional bar and message to stdout until the future resolves.
    fn progress<'a>(self, theme: impl Into<Theme<'a>>) -> ProgressFuture<'a, Self>
    where
        Self: Sized,
    {
        ProgressFuture::standalone(self, theme)
    }

    /// Wrap this future in a tracked-only [`ProgressFuture`] for inclusion in a [`Group`]. The
    /// returned value does not render on its own; the Group renders its tasks together.
    fn progressive<'a>(self) -> ProgressFuture<'a, Self>
    where
        Self: Sized,
    {
        ProgressFuture::new(self)
    }
}

impl<F> FutureExt for F where F: Future {}
