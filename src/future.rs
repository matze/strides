//! Spinner integration for futures.
//!
//! Import [`FutureExt`] to wrap any [`Future`] with a spinner, optional bar, and message via
//! [`progress()`](FutureExt::progress). The returned [`ProgressBuilder`] composes optional
//! capabilities via [`with_message()`](ProgressBuilder::with_message),
//! [`with_messages()`](ProgressBuilder::with_messages) and
//! [`with_fraction()`](ProgressBuilder::with_fraction). For multiple concurrent futures, use
//! [`Group`] which renders one line per task; see its documentation for an example.

/// Concurrent futures rendered as one line per task. See [`Group`] for the entry point.
pub mod group;

pub use group::{Group, Task};

use std::fmt::Display;
use std::future::Future;
use std::io::{IsTerminal, Write};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_lite::stream::Pending;
use futures_lite::{stream, Stream};

use crate::bar::Bar;
use crate::spinner::Ticks;
use crate::term::clear_line;
use crate::Theme;

/// Builder returned by [`FutureExt::progress`].
///
/// Holds the wrapped future together with the theme and optional capabilities — a static message, a
/// stream of dynamic messages, and a stream of progress fractions. The builder implements
/// [`Future`] so `.await` drives the rendering loop and resolves to the wrapped future's output.
///
/// The type parameters `M` and `P` track the message and fraction stream types respectively. They
/// default to [`Pending`] (a ZST that never yields) so the bare `fut.progress(theme).await` path
/// allocates nothing beyond the spinner [`Ticks`] state.
pub struct ProgressBuilder<'a, F, M, P> {
    inner: F,
    bar: Bar<'a>,
    bar_width: usize,
    ticks: Ticks<'a>,
    messages: M,
    fraction: P,
    spinner_char: Option<char>,
    message: Option<String>,
    current_fraction: f64,
    dirty: bool,
    render_buf: String,
    is_tty: bool,
}

impl<'a, F, M, P> ProgressBuilder<'a, F, M, P> {
    /// Display a static `message` while the future is pending.
    ///
    /// If [`with_messages`](Self::with_messages) is also supplied, this value is shown until the
    /// first item from the stream replaces it.
    pub fn with_message(mut self, message: impl Display) -> Self {
        self.message = Some(message.to_string());
        self.dirty = true;
        self
    }

    /// Replace the displayed message each time `messages` yields a value.
    ///
    /// When the stream is exhausted the last value remains visible. If
    /// [`with_message`](Self::with_message) was also called, its text is shown until the first
    /// stream item arrives.
    pub fn with_messages<S>(self, messages: S) -> ProgressBuilder<'a, F, S, P>
    where
        S: Stream + Unpin,
        S::Item: Display,
    {
        ProgressBuilder {
            inner: self.inner,
            bar: self.bar,
            bar_width: self.bar_width,
            ticks: self.ticks,
            messages,
            fraction: self.fraction,
            spinner_char: self.spinner_char,
            message: self.message,
            current_fraction: self.current_fraction,
            dirty: self.dirty,
            render_buf: self.render_buf,
            is_tty: self.is_tty,
        }
    }

    /// Drive the progress bar from a stream of fractions in `0.0..=1.0`.
    ///
    /// The latest value wins, so emitting at a high rate is fine. The bar's style and width are
    /// taken from the [`Theme`] passed to [`FutureExt::progress`]. If the theme has no bar
    /// configured nothing is rendered.
    pub fn with_fraction<S>(self, fraction: S) -> ProgressBuilder<'a, F, M, S>
    where
        S: Stream<Item = f64> + Unpin,
    {
        ProgressBuilder {
            inner: self.inner,
            bar: self.bar,
            bar_width: self.bar_width,
            ticks: self.ticks,
            messages: self.messages,
            fraction,
            spinner_char: self.spinner_char,
            message: self.message,
            current_fraction: self.current_fraction,
            dirty: self.dirty,
            render_buf: self.render_buf,
            is_tty: self.is_tty,
        }
    }
}

impl<F, M, P> Future for ProgressBuilder<'_, F, M, P>
where
    F: Future + Unpin,
    M: Stream + Unpin,
    M::Item: Display,
    P: Stream<Item = f64> + Unpin,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        if let Poll::Ready(spinner) = Pin::new(&mut this.ticks).poll_next(cx) {
            this.spinner_char = spinner;
            this.dirty = true;
        }

        while let Poll::Ready(Some(msg)) = Pin::new(&mut this.messages).poll_next(cx) {
            this.message = Some(msg.to_string());
            this.dirty = true;
        }

        while let Poll::Ready(Some(f)) = Pin::new(&mut this.fraction).poll_next(cx) {
            this.current_fraction = f.clamp(0.0, 1.0);
            this.dirty = true;
        }

        let item = Pin::new(&mut this.inner).poll(cx);

        if this.is_tty {
            match item {
                Poll::Pending if this.dirty => {
                    this.dirty = false;
                    let _ = clear_line(&mut std::io::stdout());

                    if let Some(spinner) = &this.spinner_char {
                        print!("{spinner} ");
                    }

                    this.render_buf.clear();
                    this.bar.render_into(
                        &mut this.render_buf,
                        this.bar_width,
                        this.current_fraction,
                    );

                    if !this.render_buf.is_empty() {
                        print!("{} ", this.render_buf);
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
        }

        item
    }
}

/// Extension trait that adds progress display to futures.
///
/// While the future is pending, a spinner, optional progress bar and message are rendered to
/// stdout. The line is cleared once the future resolves.
///
/// Import this trait and call [`progress()`](FutureExt::progress) on any pinned future to obtain a
/// [`ProgressBuilder`].
pub trait FutureExt: Future {
    /// Wrap this future in a [`ProgressBuilder`] driven by `theme`.
    ///
    /// `theme` accepts a [`Theme`] or a bare [`Spinner`](crate::spinner::Spinner) (converted via
    /// `Into`). Without further configuration, the builder renders only the spinner while the
    /// future is pending. Use [`with_message`](ProgressBuilder::with_message),
    /// [`with_messages`](ProgressBuilder::with_messages) and
    /// [`with_fraction`](ProgressBuilder::with_fraction) to add text and bar progress.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use strides::future::FutureExt;
    /// use strides::spinner::styles::DOTS_3;
    ///
    /// # futures_lite::future::block_on(async {
    /// let result = std::pin::pin!(async { 42 })
    ///     .progress(DOTS_3)
    ///     .with_message("computing …")
    ///     .await;
    /// # });
    /// ```
    fn progress<'a>(
        self,
        theme: impl Into<Theme<'a>>,
    ) -> ProgressBuilder<'a, Self, Pending<&'static str>, Pending<f64>>
    where
        Self: Sized,
    {
        let theme = theme.into();
        let bar_width = theme.effective_bar_width();

        ProgressBuilder {
            inner: self,
            bar: theme.bar,
            bar_width,
            ticks: theme.spinner.ticks(),
            messages: stream::pending(),
            fraction: stream::pending(),
            spinner_char: None,
            message: None,
            current_fraction: 0.0,
            dirty: true,
            render_buf: String::new(),
            is_tty: std::io::stdout().is_terminal(),
        }
    }

    /// Lift this future into a [`Task`] with the given static `label`, ready to be added to a
    /// [`Group`] via [`Group::push`].
    fn with_label<'a>(self, label: impl Into<String>) -> Task<'a, Self>
    where
        Self: Sized,
    {
        Task::from(self).with_label(label)
    }

    /// Lift this future into a [`Task`] with a dynamic-message stream, ready to be added to a
    /// [`Group`] via [`Group::push`].
    ///
    /// Note that this is distinct from [`ProgressBuilder::with_messages`]: the receiver here is a
    /// bare future, so the result is a [`Task`] — not a [`ProgressBuilder`].
    fn with_messages<'a, S, D>(self, messages: S) -> Task<'a, Self>
    where
        Self: Sized,
        S: Stream<Item = D> + Unpin + 'a,
        D: Display + 'a,
    {
        Task::from(self).with_messages(messages)
    }

    /// Lift this future into a [`Task`] with a per-task progress stream, ready to be added to a
    /// [`Group`] via [`Group::push`].
    fn with_progress<'a, S>(self, progress: S) -> Task<'a, Self>
    where
        Self: Sized,
        S: Stream<Item = f64> + Unpin + 'a,
    {
        Task::from(self).with_progress(progress)
    }
}

impl<F> FutureExt for F where F: Future {}
