//! Spinner integration for futures.
//!
//! Import [`FutureExt`] to wrap any [`Future`] with progress display. Two entry points:
//!
//! - [`progress(theme)`](FutureExt::progress) is sugar for `progressive().with_theme(theme)`:
//!   awaiting the returned [`ProgressFuture`] drives a spinner, optional bar and message on its
//!   own terminal line and resolves to the wrapped future's output.
//! - [`progressive()`](FutureExt::progressive) returns an unconfigured [`ProgressFuture`]. Without
//!   a [`with_theme`](ProgressFuture::with_theme) call it inherits the parent [`Group`]'s theme,
//!   with one, it overrides per-row.

pub mod group;
pub mod join;

pub use group::Group;
pub use join::{join, Join};

use std::borrow::Cow;
use std::fmt::Display;
use std::future::Future;
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
    /// A [`Future`] wrapped with progress state.
    ///
    /// `ProgressFuture` carries the wrapped future together with the shared [`Progress`] core
    /// (state read out via [`Progressive`] plus the lazily materialised standalone rendering
    /// machinery). The `M` and `P` parameters track the optional message and progress stream types
    /// and default to [`Pending`] (a ZST that never yields) so the bare `fut.progress(theme).await`
    /// path allocates nothing beyond the spinner [`Ticks`](crate::spinner::Ticks) state.
    pub struct ProgressFuture<'a, F, M = Pending<&'static str>, P = Pending<f64>> {
        #[pin]
        inner: F,
        #[pin]
        messages: M,
        #[pin]
        progress: P,
        core: Progress<'a>,
    }
}

impl<F> ProgressFuture<'_, F> {
    /// Construct a `ProgressFuture` with no theme set. Awaiting it directly renders with
    /// [`Theme::default()`]; calling [`with_theme`](Self::with_theme) overrides per-row;
    /// [`Group::push`] takes over rendering and supplies the Group's theme instead.
    pub fn new(inner: F) -> Self {
        Self {
            inner,
            messages: stream::pending(),
            progress: stream::pending(),
            core: Progress::new(),
        }
    }
}

impl<'a, F, M, P> ProgressFuture<'a, F, M, P> {
    /// Set the static label shown in the [`Label`](crate::layout::Segment::Label) segment.
    pub fn with_label(mut self, label: impl Display) -> Self {
        self.core.set_label(label.to_string());
        self
    }

    /// Replace the displayed message each time `messages` yields a value. When the stream is
    /// exhausted the last value remains visible. The item type is anything that converts into a
    /// `Cow<'static, str>`: `&'static str` and `String` are zero-copy; other formatted values
    /// should be `format!`'d at the call site.
    pub fn with_messages<S>(self, messages: S) -> ProgressFuture<'a, F, S, P>
    where
        S: Stream,
        S::Item: Into<Cow<'static, str>>,
    {
        ProgressFuture {
            inner: self.inner,
            messages,
            progress: self.progress,
            core: self.core,
        }
    }

    /// Prepend the elapsed time (seconds since the future was first polled) to the line.
    pub fn with_elapsed_time(mut self) -> Self {
        self.core.enable_elapsed_time();
        self
    }

    /// Drive the progress bar from a stream of fractions in `0.0..=1.0`. The latest value wins.
    pub fn with_progress<S>(self, progress: S) -> ProgressFuture<'a, F, M, S>
    where
        S: Stream<Item = f64>,
    {
        ProgressFuture {
            inner: self.inner,
            messages: self.messages,
            progress,
            core: self.core,
        }
    }

    /// Render this row with `theme`. Used for both the standalone path (drives the spinner /
    /// bar / cursor on its own line when awaited) and the per-row override path inside a
    /// [`Group`] (the Group consults this theme when constructing the slot's line).
    pub fn with_theme(mut self, theme: impl Into<Theme<'a>>) -> Self {
        self.core.set_theme(theme.into());
        self
    }

    /// Apply `style` to the spinner character on this row, overriding the parent
    /// [`Group`]'s default.
    pub fn with_spinner_style(mut self, style: Style) -> Self {
        self.core.set_spinner_style(style);
        self
    }

    /// Apply `style` to the annotation (label) text on this row, overriding the parent
    /// [`Group`]'s default.
    pub fn with_annotation_style(mut self, style: Style) -> Self {
        self.core.set_annotation_style(style);
        self
    }
}

impl<'a, F, M, P> Progressive<'a> for ProgressFuture<'a, F, M, P> {
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
    fn theme(&self) -> Option<&Theme<'a>> {
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

impl<F, M, P> Future for ProgressFuture<'_, F, M, P>
where
    F: Future,
    M: Stream,
    M::Item: Into<Cow<'static, str>>,
    P: Stream<Item = f64>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        if this.core.materialize() {
            // Preserve the legacy behaviour where the bar appears at 0% even with no progress stream.
            this.core.state.set_progress(0.0);
        }

        let mut dirty = this.core.tick(cx);

        while let Poll::Ready(Some(msg)) = this.messages.as_mut().poll_next(cx) {
            this.core.set_message(msg.into());
            dirty = true;
        }

        while let Poll::Ready(Some(p)) = this.progress.as_mut().poll_next(cx) {
            this.core.state.set_progress(p.clamp(0.0, 1.0));
            dirty = true;
        }

        let item = this.inner.as_mut().poll(cx);

        match item {
            Poll::Pending if dirty => this.core.render(),
            Poll::Ready(_) => this.core.clear(),
            _ => {}
        }

        item
    }
}

/// Extension trait that adds progress display to futures.
///
/// `progress` / `progressive` lift a bare future into a [`ProgressFuture`]. The setters
/// ([`with_label`](FutureExt::with_label), [`with_messages`](FutureExt::with_messages),
/// [`with_progress`](FutureExt::with_progress), [`with_elapsed_time`](FutureExt::with_elapsed_time))
/// mirror the ones on [`ProgressFuture`] and lift implicitly, so a bare future can be configured
/// and pushed into a [`Group`] without spelling out `.progressive()` first:
///
/// ```rust,no_run
/// # use std::time::{Duration, Instant};
/// # use strides::future::{FutureExt, Group};
/// # use strides::spinner;
/// # let mut group = Group::<Instant>::new(spinner::styles::DOTS_3);
/// group.push(async_io::Timer::after(Duration::from_secs(1)).with_label("fast"));
/// ```
pub trait FutureExt: Future {
    /// Wrap this future in a [`ProgressFuture`] configured for standalone rendering with `theme`.
    /// Sugar for `self.progressive().with_theme(theme)`.
    fn progress<'a>(self, theme: impl Into<Theme<'a>>) -> ProgressFuture<'a, Self>
    where
        Self: Sized,
    {
        self.progressive().with_theme(theme)
    }

    /// Wrap this future in an unconfigured [`ProgressFuture`]. Awaited directly it renders with
    /// [`Theme::default()`]; chain [`with_theme`](ProgressFuture::with_theme) for a custom theme,
    /// or push it into a [`Group`] to inherit the Group's theme.
    fn progressive<'a>(self) -> ProgressFuture<'a, Self>
    where
        Self: Sized,
    {
        ProgressFuture::new(self)
    }

    /// Lift into a [`ProgressFuture`] and attach a static label. Equivalent to
    /// `self.progressive().with_label(label)`.
    fn with_label<'a>(self, label: impl Display) -> ProgressFuture<'a, Self>
    where
        Self: Sized,
    {
        self.progressive().with_label(label)
    }

    /// Lift into a [`ProgressFuture`] and prepend elapsed time. Equivalent to
    /// `self.progressive().with_elapsed_time()`.
    fn with_elapsed_time<'a>(self) -> ProgressFuture<'a, Self>
    where
        Self: Sized,
    {
        self.progressive().with_elapsed_time()
    }

    /// Lift into a [`ProgressFuture`] and drive the displayed message from `messages`.
    /// Equivalent to `self.progressive().with_messages(messages)`.
    fn with_messages<'a, S>(self, messages: S) -> ProgressFuture<'a, Self, S>
    where
        Self: Sized,
        S: Stream,
        S::Item: Into<Cow<'static, str>>,
    {
        self.progressive().with_messages(messages)
    }

    /// Lift into a [`ProgressFuture`] and drive the progress bar from `progress`.
    /// Equivalent to `self.progressive().with_progress(progress)`.
    fn with_progress<'a, S>(self, progress: S) -> ProgressFuture<'a, Self, Pending<&'static str>, S>
    where
        Self: Sized,
        S: Stream<Item = f64>,
    {
        self.progressive().with_progress(progress)
    }
}

impl<F> FutureExt for F where F: Future {}
