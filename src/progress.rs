//! Shared progress core for the standalone (non-[`Group`](crate::future::Group)) rendering path.
//!
//! Every standalone adapter ([`ProgressFuture`](crate::future::ProgressFuture),
//! [`Join`](crate::future::Join) and the three stream wrappers) carries the same machinery: the
//! dynamic [`State`], the per-row theme / style overrides, and the lazily materialised rendering
//! bits (line, spinner ticks, cursor guard). [`Progress`] bundles all of that and owns the render
//! loop, so an adapter only has to spell out how one of its items maps onto progress and forward
//! the [`Progressive`] methods to its `core`.

use std::borrow::Cow;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_lite::Stream as _;
use owo_colors::Style;

use crate::line::{FrameContext, Line};
use crate::progressive::Progressive;
use crate::spinner::Ticks;
use crate::state::State;
use crate::term::{CursorGuard, Output};
use crate::Theme;

/// Materialised rendering bits used by the standalone path: line, spinner ticks, cursor guard.
struct Rendering {
    line: Line,
    ticks: Ticks,
    spinner_frame: Option<&'static str>,
    spinner_tick: u64,
    spinner_style: Style,
    annotation_style: Style,
    output: Output,
    is_tty: bool,
    _guard: CursorGuard,
}

/// Lifecycle of the standalone rendering bits.
enum RenderingState {
    /// Constructed but not yet polled; materialised on the first poll.
    Pending,
    /// Materialised; standalone rendering is active.
    Active(Rendering),
    /// A [`Group`](crate::future::Group) owns rendering for this row; nothing is rendered here.
    Detached,
}

/// Per-row dynamic [`State`], theme / style overrides and the standalone rendering machinery,
/// shared by every progress adapter.
pub(crate) struct Progress {
    pub(crate) state: State,
    theme_override: Option<Theme>,
    spinner_style_override: Option<Style>,
    annotation_style_override: Option<Style>,
    rendering: RenderingState,
}

impl Progress {
    pub(crate) fn new() -> Self {
        Self {
            state: State::new(),
            theme_override: None,
            spinner_style_override: None,
            annotation_style_override: None,
            rendering: RenderingState::Pending,
        }
    }

    pub(crate) fn set_label(&mut self, label: Cow<'static, str>) {
        self.state.set_label(label);
    }

    pub(crate) fn set_message(&mut self, message: Cow<'static, str>) {
        self.state.set_message(message);
    }

    pub(crate) fn enable_elapsed_time(&mut self) {
        self.state.enable_elapsed_time();
    }

    pub(crate) fn set_theme(&mut self, theme: Theme) {
        self.theme_override = Some(theme);
    }

    pub(crate) fn set_spinner_style(&mut self, style: Style) {
        self.spinner_style_override = Some(style);
    }

    pub(crate) fn set_annotation_style(&mut self, style: Style) {
        self.annotation_style_override = Some(style);
    }

    /// Materialise the standalone rendering bits on the first poll. Returns `true` exactly on the
    /// transition from [`Pending`](RenderingState::Pending) to [`Active`](RenderingState::Active),
    /// so callers can seed one-time state (such as a 0% bar). A
    /// [`Detached`](RenderingState::Detached) row owned by a `Group` stays detached and returns
    /// `false`.
    pub(crate) fn materialize(&mut self) -> bool {
        if !matches!(self.rendering, RenderingState::Pending) {
            return false;
        }
        let theme = self.theme_override.clone().unwrap_or_default();
        let output = theme.output;
        let is_tty = output.is_terminal();

        // A non-terminal output never renders, and a theme without a spinner has nothing to
        // animate; both get the never-yielding ticks so polling schedules no timer wakeups.
        let ticks = match &theme.spinner {
            Some(spinner) if is_tty => spinner.ticks(),
            _ => Ticks::never(),
        };
        let line = Line::new(&theme);
        self.rendering = RenderingState::Active(Rendering {
            line,
            ticks,
            spinner_frame: None,
            spinner_tick: 0,
            spinner_style: self.spinner_style_override.unwrap_or_default(),
            annotation_style: self.annotation_style_override.unwrap_or_default(),
            output,
            is_tty,
            _guard: CursorGuard { output, is_tty },
        });
        true
    }

    /// Advance the spinner. Returns `true` when it ticked, signalling that a repaint is warranted.
    pub(crate) fn tick(&mut self, cx: &mut Context<'_>) -> bool {
        if let RenderingState::Active(r) = &mut self.rendering {
            if let Poll::Ready(frame) = Pin::new(&mut r.ticks).poll_next(cx) {
                r.spinner_frame = frame;
                r.spinner_tick = r.spinner_tick.wrapping_add(1);
                return true;
            }
        }
        false
    }

    /// Render the current [`State`] onto the standalone line. No-op unless rendering is active.
    pub(crate) fn render(&mut self) {
        let RenderingState::Active(r) = &mut self.rendering else {
            return;
        };
        let (elapsed, show_elapsed) = if self.state.with_elapsed_time {
            (self.state.elapsed(), true)
        } else {
            (Duration::ZERO, false)
        };
        let frame = FrameContext {
            spinner_frame: r.spinner_frame,
            spinner_tick: r.spinner_tick,
            elapsed,
            show_elapsed,
            spinner_style: r.spinner_style,
            annotation_style: r.annotation_style,
        };
        r.line
            .standalone_render(&self.state, &frame, r.output, r.is_tty);
    }

    /// Clear the standalone line once work completes. No-op unless rendering is active.
    pub(crate) fn clear(&self) {
        if let RenderingState::Active(r) = &self.rendering {
            Line::standalone_clear(r.output, r.is_tty);
        }
    }
}

impl Progressive for Progress {
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
    fn theme(&self) -> Option<&Theme> {
        self.theme_override.as_ref()
    }
    fn spinner_style(&self) -> Option<Style> {
        self.spinner_style_override
    }
    fn annotation_style(&self) -> Option<Style> {
        self.annotation_style_override
    }
    fn show_elapsed_time(&self) -> bool {
        self.state.with_elapsed_time
    }
}

/// Implement [`Progressive`] for an adapter holding a
/// [`Progress`] in a field named `core`, forwarding every method. The first argument is the
/// brace-wrapped generic parameter list of the impl, the second the adapter type.
///
/// [`Join`](crate::future::Join) does not use this: its `progress()` is derived from its
/// completion count rather than read from the core.
macro_rules! forward_progressive {
    ({$($g:tt)*}, $ty:ty) => {
        impl<$($g)*> $crate::progressive::Progressive for $ty {
            fn label(&self) -> Option<&str> {
                $crate::progressive::Progressive::label(&self.core)
            }
            fn message(&self) -> Option<&str> {
                $crate::progressive::Progressive::message(&self.core)
            }
            fn progress(&self) -> Option<f64> {
                $crate::progressive::Progressive::progress(&self.core)
            }
            fn bytes_done(&self) -> u64 {
                $crate::progressive::Progressive::bytes_done(&self.core)
            }
            fn bytes_total(&self) -> Option<u64> {
                $crate::progressive::Progressive::bytes_total(&self.core)
            }
            fn rate(&self) -> Option<f64> {
                $crate::progressive::Progressive::rate(&self.core)
            }
            fn detach_rendering(&mut self) {
                $crate::progressive::Progressive::detach_rendering(&mut self.core);
            }
            fn theme(&self) -> Option<&$crate::Theme> {
                $crate::progressive::Progressive::theme(&self.core)
            }
            fn spinner_style(&self) -> Option<::owo_colors::Style> {
                $crate::progressive::Progressive::spinner_style(&self.core)
            }
            fn annotation_style(&self) -> Option<::owo_colors::Style> {
                $crate::progressive::Progressive::annotation_style(&self.core)
            }
            fn show_elapsed_time(&self) -> bool {
                $crate::progressive::Progressive::show_elapsed_time(&self.core)
            }
        }
    };
}

/// Implement the builder methods shared by every adapter holding a [`Progress`] in a field named
/// `core`: `with_label`, `with_elapsed_time`, `with_theme`, `with_spinner_style` and
/// `with_annotation_style`. Adapter-specific builders (`with_messages`, `with_progress`,
/// `with_len`) stay in the adapter's own impl block. Arguments as in [`forward_progressive`].
macro_rules! common_builders {
    ({$($g:tt)*}, $ty:ty) => {
        impl<$($g)*> $ty {
            /// Set the static label shown in the [`Label`](crate::layout::Segment::Label)
            /// segment. `&'static str` and `String` convert zero-copy; formatted values should be
            /// `format!`'d at the call site.
            pub fn with_label(
                mut self,
                label: impl Into<::std::borrow::Cow<'static, str>>,
            ) -> Self {
                self.core.set_label(label.into());
                self
            }

            /// Prepend the elapsed time (seconds since the first poll) to the line.
            pub fn with_elapsed_time(mut self) -> Self {
                self.core.enable_elapsed_time();
                self
            }

            /// Render this row with `theme`. Drives standalone rendering when polled directly;
            /// overrides the parent group's theme for this row when pushed into a group.
            pub fn with_theme(mut self, theme: impl Into<$crate::Theme>) -> Self {
                self.core.set_theme(theme.into());
                self
            }

            /// Apply `style` to the spinner character on this row, overriding the parent group's
            /// default.
            pub fn with_spinner_style(mut self, style: ::owo_colors::Style) -> Self {
                self.core.set_spinner_style(style);
                self
            }

            /// Apply `style` to the annotation (label) text on this row, overriding the parent
            /// group's default.
            pub fn with_annotation_style(mut self, style: ::owo_colors::Style) -> Self {
                self.core.set_annotation_style(style);
                self
            }
        }
    };
}

pub(crate) use {common_builders, forward_progressive};
