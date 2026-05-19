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
use std::time::Duration;

use futures_lite::stream::Pending;
use futures_lite::{stream, Stream};
use owo_colors::Style;
use pin_project_lite::pin_project;

use crate::line::{FrameContext, Line};
use crate::progressive::Progressive;
use crate::spinner::Ticks;
use crate::state::State;
use crate::term::{CursorGuard, Output};
use crate::Theme;

/// Materialised rendering bits used by the standalone path: line, spinner ticks, cursor guard.
pub(super) struct Rendering<'a> {
    pub(super) line: Line<'a>,
    pub(super) ticks: Ticks<'a>,
    pub(super) spinner_char: Option<char>,
    pub(super) spinner_style: Style,
    pub(super) annotation_style: Style,
    pub(super) output: Output,
    pub(super) is_tty: bool,
    pub(super) _guard: CursorGuard,
}

/// Lifecycle of the standalone rendering bits.
pub(super) enum RenderingState<'a> {
    /// Constructed but not yet polled.
    Pending,
    /// Standalone rendering is active.
    Active(Rendering<'a>),
    /// A [`Group`] owns rendering for this row. No standalone rendering will happen.
    Detached,
}

pin_project! {
    /// A [`Future`] wrapped with progress state.
    ///
    /// `ProgressFuture` carries the wrapped future together with state read out via [`Progressive`]
    /// and (lazily) the rendering machinery for standalone use. The `M` and `P` parameters track the
    /// optional message and progress stream types and default to [`Pending`] (a ZST that never
    /// yields) so the bare `fut.progress(theme).await` path allocates nothing beyond the spinner
    /// [`Ticks`] state.
    pub struct ProgressFuture<'a, F, M = Pending<&'static str>, P = Pending<f64>> {
        #[pin]
        inner: F,
        #[pin]
        messages: M,
        #[pin]
        progress: P,
        state: State,
        theme_override: Option<Theme<'a>>,
        spinner_style_override: Option<Style>,
        annotation_style_override: Option<Style>,
        rendering: RenderingState<'a>,
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
            state: State::new(),
            theme_override: None,
            spinner_style_override: None,
            annotation_style_override: None,
            rendering: RenderingState::Pending,
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
            state: self.state,
            theme_override: self.theme_override,
            spinner_style_override: self.spinner_style_override,
            annotation_style_override: self.annotation_style_override,
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
        S: Stream<Item = f64>,
    {
        ProgressFuture {
            inner: self.inner,
            messages: self.messages,
            progress,
            state: self.state,
            theme_override: self.theme_override,
            spinner_style_override: self.spinner_style_override,
            annotation_style_override: self.annotation_style_override,
            rendering: self.rendering,
        }
    }

    /// Render this row with `theme`. Used for both the standalone path (drives the spinner /
    /// bar / cursor on its own line when awaited) and the per-row override path inside a
    /// [`Group`] (the Group consults this theme when constructing the slot's line).
    pub fn with_theme(mut self, theme: impl Into<Theme<'a>>) -> Self {
        self.theme_override = Some(theme.into());
        self
    }

    /// Apply `style` to the spinner character on this row, overriding the parent
    /// [`Group`]'s default.
    pub fn with_spinner_style(mut self, style: Style) -> Self {
        self.spinner_style_override = Some(style);
        self
    }

    /// Apply `style` to the annotation (label) text on this row, overriding the parent
    /// [`Group`]'s default.
    pub fn with_annotation_style(mut self, style: Style) -> Self {
        self.annotation_style_override = Some(style);
        self
    }
}

impl<'a, F, M, P> Progressive<'a> for ProgressFuture<'a, F, M, P> {
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

        if matches!(this.rendering, RenderingState::Pending) {
            let theme = this.theme_override.clone().unwrap_or_default();
            let output = theme.output;
            let is_tty = output.is_terminal();
            let ticks = theme.spinner.ticks();
            let line = Line::new(&theme);
            // Preserve the legacy behaviour where the bar appears at 0% even with no progress stream.
            this.state.set_progress(0.0);
            *this.rendering = RenderingState::Active(Rendering {
                line,
                ticks,
                spinner_char: None,
                spinner_style: this.spinner_style_override.unwrap_or_default(),
                annotation_style: this.annotation_style_override.unwrap_or_default(),
                output,
                is_tty,
                _guard: CursorGuard { output, is_tty },
            });
        }

        let mut dirty = false;

        if let RenderingState::Active(r) = &mut this.rendering {
            if let Poll::Ready(ch) = Pin::new(&mut r.ticks).poll_next(cx) {
                r.spinner_char = ch;
                dirty = true;
            }
        }

        while let Poll::Ready(Some(msg)) = this.messages.as_mut().poll_next(cx) {
            this.state.set_message(msg.into());
            dirty = true;
        }

        while let Poll::Ready(Some(p)) = this.progress.as_mut().poll_next(cx) {
            this.state.set_progress(p.clamp(0.0, 1.0));
            dirty = true;
        }

        let item = this.inner.as_mut().poll(cx);

        if let RenderingState::Active(r) = &mut this.rendering {
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
                    r.line.standalone_render(this.state, &frame, r.output, r.is_tty);
                }
                Poll::Ready(_) => Line::standalone_clear(r.output, r.is_tty),
                _ => {}
            }
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
