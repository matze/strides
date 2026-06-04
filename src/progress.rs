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
struct Rendering<'a> {
    line: Line<'a>,
    ticks: Ticks<'a>,
    spinner_frame: Option<&'a str>,
    spinner_tick: u64,
    spinner_style: Style,
    annotation_style: Style,
    output: Output,
    is_tty: bool,
    _guard: CursorGuard,
}

/// Lifecycle of the standalone rendering bits.
enum RenderingState<'a> {
    /// Constructed but not yet polled; materialised on the first poll.
    Pending,
    /// Materialised; standalone rendering is active.
    Active(Rendering<'a>),
    /// A [`Group`](crate::future::Group) owns rendering for this row; nothing is rendered here.
    Detached,
}

/// Per-row dynamic [`State`], theme / style overrides and the standalone rendering machinery,
/// shared by every progress adapter.
pub(crate) struct Progress<'a> {
    pub(crate) state: State,
    theme_override: Option<Theme<'a>>,
    spinner_style_override: Option<Style>,
    annotation_style_override: Option<Style>,
    rendering: RenderingState<'a>,
}

impl<'a> Progress<'a> {
    pub(crate) fn new() -> Self {
        Self {
            state: State::new(),
            theme_override: None,
            spinner_style_override: None,
            annotation_style_override: None,
            rendering: RenderingState::Pending,
        }
    }

    pub(crate) fn set_label(&mut self, label: String) {
        self.state.set_label(label);
    }

    pub(crate) fn set_message(&mut self, message: Cow<'static, str>) {
        self.state.set_message(message);
    }

    pub(crate) fn enable_elapsed_time(&mut self) {
        self.state.enable_elapsed_time();
    }

    pub(crate) fn set_theme(&mut self, theme: Theme<'a>) {
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
        let ticks = theme.spinner.ticks();
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

impl<'a> Progressive<'a> for Progress<'a> {
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
    fn show_elapsed_time(&self) -> bool {
        self.state.with_elapsed_time
    }
}
