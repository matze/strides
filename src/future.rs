//! Spinner integration for futures.
//!
//! Import [`FutureExt`] to wrap any [`Future`] with a spinner, optional bar, and message via
//! [`progress()`](FutureExt::progress). The returned [`ProgressBuilder`] composes optional
//! capabilities via [`with_label()`](ProgressBuilder::with_label),
//! [`with_messages()`](ProgressBuilder::with_messages) and
//! [`with_progress()`](ProgressBuilder::with_progress). For multiple concurrent futures, use
//! [`Group`] which renders one line per task; see its documentation for an example.

/// Concurrent futures rendered as one line per task. See [`Group`] for the entry point.
pub mod group;

pub use group::{Group, Task};

use std::fmt::Display;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_lite::stream::Pending;
use futures_lite::{stream, Stream};

use crate::state::State;
use crate::Theme;

/// Builder returned by [`FutureExt::progress`].
///
/// Holds the wrapped future together with the theme and optional capabilities — a static label, a
/// stream of dynamic messages, and a stream of progress fractions. The builder implements
/// [`Future`] so `.await` drives the rendering loop and resolves to the wrapped future's output.
///
/// The type parameters `M` and `P` track the message and progress stream types respectively. They
/// default to [`Pending`] (a ZST that never yields) so the bare `fut.progress(theme).await` path
/// allocates nothing beyond the spinner [`Ticks`](crate::spinner::Ticks) state.
pub struct ProgressBuilder<'a, F, M, P> {
    inner: F,
    messages: M,
    progress: P,
    state: State<'a>,
}

impl<'a, F, M, P> ProgressBuilder<'a, F, M, P> {
    /// Display a static `label` while the future is pending.
    ///
    /// If [`with_messages`](Self::with_messages) is also supplied, this value is shown until the
    /// first item from the stream replaces it.
    pub fn with_label(mut self, label: impl Display) -> Self {
        self.state.set_message(label.to_string());
        self
    }

    /// Replace the displayed message each time `messages` yields a value.
    ///
    /// When the stream is exhausted the last value remains visible. If
    /// [`with_label`](Self::with_label) was also called, its text is shown until the first
    /// stream item arrives.
    pub fn with_messages<S>(self, messages: S) -> ProgressBuilder<'a, F, S, P>
    where
        S: Stream + Unpin,
        S::Item: Display,
    {
        ProgressBuilder {
            inner: self.inner,
            messages,
            progress: self.progress,
            state: self.state,
        }
    }

    /// Prepend the elapsed time (seconds since the future was first polled) to the line.
    pub fn with_elapsed_time(mut self) -> Self {
        self.state.enable_elapsed_time();
        self
    }

    /// Drive the progress bar from a stream of fractions in `0.0..=1.0`.
    ///
    /// The latest value wins, so emitting at a high rate is fine. The bar's style and width are
    /// taken from the [`Theme`] passed to [`FutureExt::progress`]. If the theme has no bar
    /// configured nothing is rendered.
    pub fn with_progress<S>(self, progress: S) -> ProgressBuilder<'a, F, M, S>
    where
        S: Stream<Item = f64> + Unpin,
    {
        ProgressBuilder {
            inner: self.inner,
            messages: self.messages,
            progress,
            state: self.state,
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

        this.state.poll_spinner(cx);

        while let Poll::Ready(Some(msg)) = Pin::new(&mut this.messages).poll_next(cx) {
            this.state.set_message(msg.to_string());
        }

        while let Poll::Ready(Some(f)) = Pin::new(&mut this.progress).poll_next(cx) {
            this.state.set_progress(f.clamp(0.0, 1.0));
        }

        let item = Pin::new(&mut this.inner).poll(cx);

        match item {
            Poll::Pending => this.state.render_if_dirty(),
            Poll::Ready(_) => this.state.finish(),
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
    /// future is pending. Use [`with_label`](ProgressBuilder::with_label),
    /// [`with_messages`](ProgressBuilder::with_messages) and
    /// [`with_progress`](ProgressBuilder::with_progress) to add text and bar progress.
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
    ///     .with_label("computing …")
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
        let mut state = State::new(theme.into());
        // Preserve the previous behaviour where the bar is always present (starting at 0%) even
        // when no progress stream is attached.
        state.set_progress(0.0);

        ProgressBuilder {
            inner: self,
            messages: stream::pending(),
            progress: stream::pending(),
            state,
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
