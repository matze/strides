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
use std::pin::Pin;
use std::task::Poll;

use futures_lite::stream::Pending;
use futures_lite::{stream, Stream};

use crate::state::State;
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
    fraction_fn: F,
    messages: M,
    current: usize,
    state: State<'a>,
}

impl<'a, S, F, M> StreamProgressBuilder<'a, S, F, M> {
    /// Display a static `label` while items flow through.
    ///
    /// If [`with_messages`](Self::with_messages) is also supplied, this value is shown until the
    /// first item from the stream replaces it.
    pub fn with_label(mut self, label: impl Display) -> Self {
        self.state.set_message(label.to_string());
        self
    }

    /// Prepend `[Xs]` (seconds since the first item flowed through) to the line.
    pub fn with_elapsed_time(mut self) -> Self {
        self.state.enable_elapsed_time();
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
            fraction_fn: self.fraction_fn,
            messages,
            current: self.current,
            state: self.state,
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

        this.state.poll_spinner(cx);

        while let Poll::Ready(Some(msg)) = Pin::new(&mut this.messages).poll_next(cx) {
            this.state.set_message(msg.to_string());
        }

        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(item)) => {
                this.current += 1;
                let completed = (this.fraction_fn)(this.current, &item);
                this.state.set_progress(completed);
                this.state.render_now();
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                this.state.finish();
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
        StreamProgressBuilder {
            inner: self,
            fraction_fn,
            messages: stream::pending(),
            current: 0,
            state: State::new(theme.into()),
        }
    }
}

impl<S> StreamExt for S where S: Stream {}
