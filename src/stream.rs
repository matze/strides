//! Progress bar extension for streams.

use std::io::Write;
use std::pin::Pin;
use std::task::Poll;

use futures_lite::{Stream, stream};

use crate::bar::Bar;
use crate::term::clear_line;

pub use crate::style::ProgressStyle;

/// Stream for the [`progress`](StreamExt::progress) and
/// [`progress_with_messages`](StreamExt::progress_with_messages) methods.
pub struct Progress<'a, S, F, T, M> {
    /// Wrapped stream.
    inner: S,
    /// Progress bar style
    bar: Bar<'a>,
    /// Width of the progress bar in characters.
    bar_width: usize,
    /// Closure to compute the progress.
    progress_fn: F,
    /// Spinner tick stream.
    ticks: T,
    /// Messages stream.
    messages: M,
    /// Current index
    current: usize,
    /// Current spinner character.
    spinner: Option<char>,
    /// Current message.
    message: Option<String>,
}

impl<'a, S, F, T, M, D> Stream for Progress<'a, S, F, T, M>
where
    S: Stream + Unpin,
    F: FnMut(usize, &S::Item) -> f64 + Unpin,
    T: Stream<Item = char> + Unpin,
    M: Stream<Item = D> + Unpin,
    D: std::fmt::Display,
{
    type Item = S::Item;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let inner = Pin::new(&mut this.inner);
        let ticks = Pin::new(&mut this.ticks);
        let messages = Pin::new(&mut this.messages);

        // Poll the spinner stream.
        if let Poll::Ready(spinner) = ticks.poll_next(cx) {
            this.spinner = spinner;
        }

        // Poll the message stream.
        if let Poll::Ready(Some(message)) = messages.poll_next(cx) {
            this.message = Some(message.to_string());
        }

        // Poll the wrapped stream.
        match inner.poll_next(cx) {
            Poll::Ready(Some(item)) => {
                this.current += 1;

                let _ = clear_line(&mut std::io::stdout());

                if let Some(spinner) = &this.spinner {
                    print!("{spinner} ");
                }

                let completed = (this.progress_fn)(this.current, &item);

                print!("{}", this.bar.render(this.bar_width, completed));

                if let Some(message) = &this.message {
                    print!(" {message}");
                }

                std::io::stdout().flush().expect("flushing");
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                // Stream ended, so clear output.
                let _ = clear_line(&mut std::io::stdout());
                std::io::stdout().flush().expect("flushing");
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
/// Import this trait and call [`progress()`](StreamExt::progress) or
/// [`progress_with_messages()`](StreamExt::progress_with_messages) on any
/// stream.
pub trait StreamExt<'a, F>: Stream {
    /// Display a progress bar while consuming this stream.
    ///
    /// `progress_fn` is called for every item and must return a value between `0.0` (no progress)
    /// and `1.0` (complete). It receives the monotonically increasing item index (starting at 1)
    /// and a reference to the item, so progress can be derived from either the count or the item
    /// content.
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
    ///     .progress(DOTS_3.into(), move |i, _| i as f64 / total as f64)
    ///     .count()
    ///     .await;
    /// # });
    /// ```
    fn progress(
        self,
        progress: ProgressStyle<'a>,
        progress_fn: F,
    ) -> Progress<
        'a,
        Self,
        F,
        impl Stream<Item = char> + use<'a, F, Self>,
        impl Stream<Item = impl std::fmt::Display>,
    >
    where
        Self: Sized,
        F: FnMut(usize, &Self::Item) -> f64 + Unpin,
    {
        let bar_width = progress.effective_bar_width();

        Progress {
            inner: self,
            progress_fn,
            bar: progress.bar,
            bar_width,
            ticks: progress.spinner.ticks(),
            messages: stream::pending::<&'static str>(),
            current: 0,
            spinner: None,
            message: None,
        }
    }

    /// Display a progress bar with dynamically changing messages while consuming this stream.
    ///
    /// Works like [`progress()`](StreamExt::progress) but takes an additional `messages` stream.
    /// Each time a new message arrives it replaces the text shown after the progress bar. If the
    /// message stream is exhausted before the wrapped stream completes, the last message remains
    /// visible.
    fn progress_with_messages(
        self,
        progress: ProgressStyle<'a>,
        progress_fn: F,
        messages: impl Stream<Item = impl std::fmt::Display>,
    ) -> Progress<'a, Self, F, impl Stream<Item = char>, impl Stream<Item = impl std::fmt::Display>>
    where
        Self: Sized,
        F: FnMut(usize, &Self::Item) -> f64 + Unpin,
    {
        let bar_width = progress.effective_bar_width();

        Progress {
            inner: self,
            progress_fn,
            bar: progress.bar,
            bar_width,
            ticks: progress.spinner.ticks(),
            messages,
            current: 0,
            spinner: None,
            message: None,
        }
    }
}

impl<'a, S, F> StreamExt<'a, F> for S where S: Stream {}
