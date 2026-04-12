//! Spinner integration for futures.

pub mod group;

pub use group::Group;

use std::future::Future;
use std::io::Write;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_lite::{FutureExt as _, Stream, stream};

use crate::bar::Bar;
use crate::style::ProgressStyle;
use crate::term::clear_line;

/// Future for the [`progress`](FutureExt::progress) and
/// [`progress_with_messages`](FutureExt::progress_with_messages) methods.
pub struct Progress<'a, F, T, M> {
    /// Wrapped future.
    inner: F,
    /// Spinner tick stream.
    ticks: T,
    /// Messages stream.
    messages: M,
    /// Progress bar style.
    bar: Bar<'a>,
    /// Width of the progress bar in characters.
    bar_width: usize,
    /// Current annotation for the future.
    message: Option<String>,
    /// Current spinner character.
    spinner: Option<char>,
    /// Whether the display needs to be redrawn.
    dirty: bool,
}

impl<F, T, M, D> Future for Progress<'_, F, T, M>
where
    F: Future + Unpin,
    T: Stream<Item = char> + Unpin,
    M: Stream<Item = D> + Unpin,
    D: std::fmt::Display,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let ticks = Pin::new(&mut this.ticks);

        if let Poll::Ready(spinner) = ticks.poll_next(cx) {
            this.spinner = spinner;
            this.dirty = true;
        }

        let messages = Pin::new(&mut this.messages);

        if let Poll::Ready(Some(message)) = messages.poll_next(cx) {
            this.message = Some(message.to_string());
            this.dirty = true;
        }

        let item = this.inner.poll(cx);

        match item {
            Poll::Pending if this.dirty => {
                this.dirty = false;
                let _ = clear_line(&mut std::io::stdout());

                if let Some(spinner) = &this.spinner {
                    print!("{spinner} ");
                }

                let bar = this.bar.render(this.bar_width, 0.0);

                if !bar.is_empty() {
                    print!("{bar} ");
                }

                if let Some(message) = &this.message {
                    print!("{message}");
                }

                std::io::stdout().flush().expect("flushing");
            }
            Poll::Ready(_) => {
                let _ = clear_line(&mut std::io::stdout());
                std::io::stdout().flush().expect("flushing");
            }
            _ => {}
        }

        item
    }
}

/// Extension trait that adds progress display to futures.
///
/// While the future is pending, a spinner, optional progress bar and message are rendered to
/// stdout. The line is cleared once the future resolves.
///
/// Import this trait and call [`progress()`](FutureExt::progress) or
/// [`progress_with_messages()`](FutureExt::progress_with_messages) on any pinned future.
pub trait FutureExt: Future {
    /// Display a spinner and a static message while this future is pending.
    ///
    /// `style` accepts a [`ProgressStyle`] or a bare [`Spinner`](crate::spinner::Spinner)
    /// (converted via `Into`).  When the style includes a bar it is rendered between the spinner
    /// and the message.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use strides::future::FutureExt;
    /// use strides::spinner::styles::DOTS_3;
    ///
    /// # futures_lite::future::block_on(async {
    /// let result = std::pin::pin!(async { 42 })
    ///     .progress(DOTS_3, "computing …")
    ///     .await;
    /// # });
    /// ```
    fn progress<'a>(
        self,
        style: impl Into<ProgressStyle<'a>>,
        message: impl std::fmt::Display,
    ) -> Progress<'a, Self, impl Stream<Item = char>, impl Stream<Item = impl std::fmt::Display>>
    where
        Self: Sized,
    {
        let style = style.into();
        let bar_width = style.effective_bar_width();

        Progress {
            inner: self,
            ticks: style.spinner.ticks(),
            messages: stream::pending::<&'static str>(),
            bar: style.bar,
            bar_width,
            message: Some(message.to_string()),
            spinner: None,
            dirty: true,
        }
    }

    /// Display a spinner with dynamically changing messages while this future is pending.
    ///
    /// `messages` is a stream of displayable values. Each time a new value arrives it replaces the
    /// currently shown message. If the message stream is exhausted before the future completes, the
    /// last message remains visible.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use futures_lite::stream;
    /// use strides::future::FutureExt;
    /// use strides::spinner::styles::DOTS_3;
    ///
    /// # futures_lite::future::block_on(async {
    /// let messages = stream::iter(["connecting …", "fetching …", "done"]);
    /// let result = std::pin::pin!(async { 42 })
    ///     .progress_with_messages(DOTS_3, messages)
    ///     .await;
    /// # });
    /// ```
    fn progress_with_messages<'a>(
        self,
        style: impl Into<ProgressStyle<'a>>,
        messages: impl Stream<Item = impl std::fmt::Display>,
    ) -> Progress<'a, Self, impl Stream<Item = char>, impl Stream<Item = impl std::fmt::Display>>
    where
        Self: Sized,
    {
        let style = style.into();
        let bar_width = style.effective_bar_width();

        Progress {
            inner: self,
            ticks: style.spinner.ticks(),
            messages,
            bar: style.bar,
            bar_width,
            message: None,
            spinner: None,
            dirty: true,
        }
    }
}

impl<F> FutureExt for F where F: Future {}
