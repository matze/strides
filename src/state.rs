//! Shared rendering scaffolding used by the future, stream and I/O progress wrappers.
//!
//! [`State`] owns the static configuration drawn from a [`Theme`] (bar, spinner ticks, layout,
//! cursor guard) together with the dynamic values mutated as work flows through (current spinner
//! char, message, progress fraction, elapsed-time start).

use std::io::{IsTerminal, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use futures_lite::Stream;

use crate::bar::Bar;
use crate::layout::{Layout, RenderContext};
use crate::spinner::Ticks;
use crate::term::{self, clear_line, CursorGuard};
use crate::Theme;

/// Shared progress-rendering state.
///
/// Wrappers compose this and drive it via the `set_*` / `poll_spinner` / `render_*` helpers. The
/// `dirty` flag is updated automatically by the helpers; callers only need to decide whether to
/// render unconditionally ([`render_now`](Self::render_now)) or only when something actually
/// changed ([`render_if_dirty`](Self::render_if_dirty)).
pub(crate) struct State<'a> {
    bar: Bar<'a>,
    bar_width: usize,
    ticks: Ticks<'a>,
    layout: Layout,
    guard: CursorGuard,
    spinner_char: Option<char>,
    message: Option<String>,
    progress: Option<f64>,
    with_elapsed_time: bool,
    start: Option<Instant>,
    render_buf: String,
    dirty: bool,
}

impl<'a> State<'a> {
    /// Build a new state from `theme`, with no progress fraction set, elapsed time disabled and
    /// `dirty` set so the first render call will draw.
    pub(crate) fn new(theme: Theme<'a>) -> Self {
        let bar_width = theme.effective_bar_width();
        Self {
            bar: theme.bar,
            bar_width,
            ticks: theme.spinner.ticks(),
            layout: theme.layout,
            guard: CursorGuard {
                is_tty: std::io::stdout().is_terminal(),
            },
            spinner_char: None,
            message: None,
            progress: None,
            with_elapsed_time: false,
            start: None,
            render_buf: String::new(),
            dirty: true,
        }
    }

    /// Replace the displayed message and mark the state dirty.
    pub(crate) fn set_message(&mut self, message: String) {
        self.message = Some(message);
        self.dirty = true;
    }

    /// Set the progress fraction and mark the state dirty.
    pub(crate) fn set_progress(&mut self, progress: f64) {
        self.progress = Some(progress);
        self.dirty = true;
    }

    /// Enable rendering of the elapsed time. The `start` instant is captured lazily on the first
    /// render so the displayed elapsed time matches "since first frame", not "since builder
    /// construction".
    pub(crate) fn enable_elapsed_time(&mut self) {
        self.with_elapsed_time = true;
        self.dirty = true;
    }

    /// Poll the spinner tick stream once and store the latest character. Marks the state dirty when
    /// a new character arrives.
    pub(crate) fn poll_spinner(&mut self, cx: &mut Context<'_>) {
        if let Poll::Ready(spinner) = Pin::new(&mut self.ticks).poll_next(cx) {
            self.spinner_char = spinner;
            self.dirty = true;
        }
    }

    /// Render only when something has changed since the last render. Clears the dirty flag.
    pub(crate) fn render_if_dirty(&mut self) {
        if self.dirty {
            self.render_now();
        }
    }

    /// Render unconditionally. Clears the dirty flag.
    pub(crate) fn render_now(&mut self) {
        self.dirty = false;

        if !self.guard.is_tty {
            return;
        }

        let elapsed = if self.with_elapsed_time {
            self.start.get_or_insert_with(Instant::now).elapsed()
        } else {
            Duration::ZERO
        };

        let ctx = RenderContext {
            spinner: self.spinner_char,
            elapsed,
            show_elapsed: self.with_elapsed_time,
            bar: &self.bar,
            bar_width: self.bar_width,
            progress: self.progress,
            label: None,
            message: self.message.as_deref(),
            spinner_style: owo_colors::Style::new(),
            annotation_style: owo_colors::Style::new(),
        };

        self.render_buf.clear();
        self.layout.render(&ctx, &mut self.render_buf);

        let mut stdout = std::io::stdout().lock();
        let _ = clear_line(&mut stdout);
        let _ = stdout.write_all(term::HIDE_CURSOR);
        let _ = stdout.write_all(self.render_buf.as_bytes());
        let _ = stdout.flush();
    }

    /// Clear the line and restore the cursor. Call this when the wrapped work finishes.
    pub(crate) fn finish(&self) {
        if !self.guard.is_tty {
            return;
        }

        let mut stdout = std::io::stdout().lock();
        let _ = clear_line(&mut stdout);
        let _ = stdout.write_all(term::SHOW_CURSOR);
        let _ = stdout.flush();
    }
}
