//! Progress bar extension for streams.
//!
//! Import [`StreamExt`] to call [`progress()`](StreamExt::progress) on any [`Stream`]. The fraction
//! closure receives the running item index (starting at 1) and a reference to the item, so the
//! fraction can be derived either from a known total or from the item itself (e.g. accumulated
//! bytes / `Content-Length`). See `examples/rget.rs` for a download progress bar driven by the
//! latter.
//!
//! Dynamic messages compose on top of the returned [`StreamProgressBuilder`] via
//! [`with_messages`](StreamProgressBuilder::with_messages).

use std::fmt::Display;
use std::io::{IsTerminal, Write};
use std::pin::Pin;
use std::task::Poll;
use std::time::{Duration, Instant};

use futures_lite::stream::Pending;
use futures_lite::{stream, Stream};

use crate::bar::Bar;
use crate::layout::{Layout, RenderContext};
use crate::spinner::Ticks;
use crate::term::{self, clear_line, CursorGuard};
use crate::Theme;

/// Builder returned by [`StreamExt::progress`].
///
/// Wraps the inner stream and drives a spinner, progress bar and optional message line as items
/// flow through. The builder itself implements [`Stream`] so the wrapped items are passed through
/// unchanged.
///
/// The `M` parameter tracks the optional messages stream and defaults to [`Pending`] (a ZST that
/// never yields).
pub struct StreamProgressBuilder<'a, S, F, M> {
    inner: S,
    bar: Bar<'a>,
    bar_width: usize,
    ticks: Ticks<'a>,
    fraction_fn: F,
    messages: M,
    current: usize,
    spinner_char: Option<char>,
    message: Option<String>,
    with_elapsed_time: bool,
    start: Option<Instant>,
    render_buf: String,
    layout: Layout,
    guard: CursorGuard,
}

impl<'a, S, F, M> StreamProgressBuilder<'a, S, F, M> {
    /// Display a static `label` while items flow through.
    ///
    /// If [`with_messages`](Self::with_messages) is also supplied, this value is shown until the
    /// first item from the stream replaces it.
    pub fn with_label(mut self, label: impl Display) -> Self {
        self.message = Some(label.to_string());
        self
    }

    /// Prepend `[Xs]` (seconds since the first item flowed through) to the line.
    pub fn with_elapsed_time(mut self) -> Self {
        self.with_elapsed_time = true;
        self
    }

    /// Replace the displayed message each time `messages` yields a value.
    ///
    /// When the stream is exhausted the last value remains visible.
    pub fn with_messages<S2>(self, messages: S2) -> StreamProgressBuilder<'a, S, F, S2>
    where
        S2: Stream + Unpin,
        S2::Item: Display,
    {
        StreamProgressBuilder {
            inner: self.inner,
            bar: self.bar,
            bar_width: self.bar_width,
            ticks: self.ticks,
            fraction_fn: self.fraction_fn,
            messages,
            current: self.current,
            spinner_char: self.spinner_char,
            message: self.message,
            with_elapsed_time: self.with_elapsed_time,
            start: self.start,
            render_buf: self.render_buf,
            layout: self.layout,
            guard: self.guard,
        }
    }
}

impl<S, F, M> Stream for StreamProgressBuilder<'_, S, F, M>
where
    S: Stream + Unpin,
    F: FnMut(usize, &S::Item) -> f64 + Unpin,
    M: Stream + Unpin,
    M::Item: Display,
{
    type Item = S::Item;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if let Poll::Ready(spinner) = Pin::new(&mut this.ticks).poll_next(cx) {
            this.spinner_char = spinner;
        }

        while let Poll::Ready(Some(msg)) = Pin::new(&mut this.messages).poll_next(cx) {
            this.message = Some(msg.to_string());
        }

        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(item)) => {
                this.current += 1;

                if this.guard.is_tty {
                    let elapsed = if this.with_elapsed_time {
                        this.start.get_or_insert_with(Instant::now).elapsed()
                    } else {
                        Duration::ZERO
                    };

                    let completed = (this.fraction_fn)(this.current, &item);

                    let ctx = RenderContext {
                        spinner: this.spinner_char,
                        elapsed,
                        show_elapsed: this.with_elapsed_time,
                        bar: &this.bar,
                        bar_width: this.bar_width,
                        progress: Some(completed),
                        label: None,
                        message: this.message.as_deref(),
                        spinner_style: owo_colors::Style::new(),
                        annotation_style: owo_colors::Style::new(),
                    };

                    this.render_buf.clear();
                    this.layout.render(&ctx, &mut this.render_buf);

                    let mut stdout = std::io::stdout().lock();
                    let _ = clear_line(&mut stdout);
                    let _ = stdout.write_all(term::HIDE_CURSOR);
                    let _ = stdout.write_all(this.render_buf.as_bytes());
                    let _ = stdout.flush();
                }

                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                if this.guard.is_tty {
                    let mut stdout = std::io::stdout().lock();
                    let _ = clear_line(&mut stdout);
                    let _ = stdout.write_all(term::SHOW_CURSOR);
                    let _ = stdout.flush();
                }
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Extension trait that adds progress display to streams.
///
/// Each time the wrapped stream yields an item, a spinner, progress bar and optional message are
/// rendered to stdout. The line is cleared when the stream ends.
///
/// Import this trait and call [`progress()`](StreamExt::progress) on any stream to obtain a
/// [`StreamProgressBuilder`].
pub trait StreamExt: Stream {
    /// Wrap this stream in a [`StreamProgressBuilder`] driven by `theme`.
    ///
    /// `theme` accepts a [`Theme`] or a bare [`Spinner`](crate::spinner::Spinner) (converted via
    /// `Into`). `fraction_fn` is called for every item and must return a value between `0.0` (no
    /// progress) and `1.0` (complete). It receives the monotonically increasing item index
    /// (starting at 1) and a reference to the item, so progress can be derived from either the
    /// count or the item content.
    ///
    /// Use [`with_messages`](StreamProgressBuilder::with_messages) on the returned builder to also
    /// display dynamic messages.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use futures_lite::StreamExt as _;
    /// use strides::stream::StreamExt;
    /// use strides::spinner::styles::DOTS_3;
    ///
    /// # futures_lite::future::block_on(async {
    /// let total = 100;
    /// futures_lite::stream::iter(0..total)
    ///     .progress(DOTS_3, move |i, _| i as f64 / total as f64)
    ///     .count()
    ///     .await;
    /// # });
    /// ```
    fn progress<'a, F>(
        self,
        theme: impl Into<Theme<'a>>,
        fraction_fn: F,
    ) -> StreamProgressBuilder<'a, Self, F, Pending<&'static str>>
    where
        Self: Sized,
        F: FnMut(usize, &Self::Item) -> f64 + Unpin,
    {
        let theme = theme.into();
        let bar_width = theme.effective_bar_width();

        StreamProgressBuilder {
            inner: self,
            bar: theme.bar,
            bar_width,
            ticks: theme.spinner.ticks(),
            fraction_fn,
            messages: stream::pending(),
            current: 0,
            spinner_char: None,
            message: None,
            with_elapsed_time: false,
            start: None,
            render_buf: String::new(),
            layout: theme.layout,
            guard: CursorGuard {
                is_tty: std::io::stdout().is_terminal(),
            },
        }
    }
}

impl<S> StreamExt for S where S: Stream {}
