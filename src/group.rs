//! Shared rendering core for the multi-line Groups.
//!
//! [`GroupCore`] owns everything the two Groups ([`future::Group`](crate::future::Group) and
//! [`stream::Group`](crate::stream::Group)) have in common: the default theme, the spinner ticks,
//! group-wide style defaults, output selection and the repaint loop. The Groups themselves keep
//! only their slot storage and poll semantics (a future resolves once, a stream yields many
//! items).

use std::fmt::Write as _;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use futures_lite::Stream as _;
use owo_colors::Style;

use crate::line::{FrameContext, Line};
use crate::progressive::Progressive;
use crate::spinner::Ticks;
use crate::term::{self, CursorGuard, Output};
use crate::Theme;

pub(crate) struct GroupCore {
    /// Default theme for rows that don't supply their own.
    pub(crate) theme: Theme,
    /// Default spinner style for rows that don't supply their own.
    pub(crate) spinner_style: Style,
    /// Default annotation (label) style for rows that don't supply their own.
    pub(crate) annotation_style: Style,
    /// Default for showing elapsed time on rows that don't opt in themselves.
    pub(crate) with_elapsed_time: bool,
    ticks: Ticks,
    spinner_frame: Option<&'static str>,
    spinner_tick: u64,
    /// Instant of the first poll, driving the elapsed-time segment.
    start: Option<Instant>,
    output: Output,
    is_tty: bool,
    /// Rows drawn by the previous repaint, so completed rows are cleared rather than left behind.
    rendered_lines: usize,
    dirty: bool,
    /// Frame buffer reused across repaints: the entire frame — escapes and all rows — is
    /// assembled here and written in a single call, minimizing syscalls (stderr is unbuffered,
    /// stdout flushes per newline) and avoiding tearing.
    frame_buf: String,
    _guard: CursorGuard,
}

impl GroupCore {
    pub(crate) fn new(theme: Theme) -> Self {
        let output = theme.output;
        let is_tty = output.is_terminal();

        // A non-terminal output never renders, and a theme without a spinner has nothing to
        // animate; both get the never-yielding ticks so polling schedules no timer wakeups.
        let ticks = match &theme.spinner {
            Some(spinner) if is_tty => spinner.ticks(),
            _ => Ticks::never(),
        };

        Self {
            theme,
            spinner_style: Style::new(),
            annotation_style: Style::new(),
            with_elapsed_time: false,
            ticks,
            spinner_frame: None,
            spinner_tick: 0,
            start: None,
            output,
            is_tty,
            rendered_lines: 0,
            dirty: true,
            frame_buf: String::new(),
            _guard: CursorGuard { output, is_tty },
        }
    }

    /// Flag that the next repaint has something new to draw (a row was added, yielded an item or
    /// completed).
    pub(crate) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Note the instant of the first poll and advance the spinner.
    pub(crate) fn tick(&mut self, cx: &mut Context<'_>) {
        self.start.get_or_insert_with(Instant::now);

        if let Poll::Ready(frame) = Pin::new(&mut self.ticks).poll_next(cx) {
            self.spinner_frame = frame;
            self.spinner_tick = self.spinner_tick.wrapping_add(1);
            self.dirty = true;
        }
    }

    /// Repaint one line per row, clear lines left over from rows that completed since the last
    /// frame, and move the cursor back up. The whole frame is assembled into the reusable buffer
    /// and written in a single call. No-op unless the output is a terminal and something changed.
    /// `active_count` must equal the number of items `rows` yields.
    pub(crate) fn repaint<'i, P>(
        &mut self,
        active_count: usize,
        rows: impl IntoIterator<Item = (&'i Line, &'i P)>,
    ) where
        P: Progressive + ?Sized + 'i,
    {
        if !(self.is_tty && self.dirty && (active_count > 0 || self.rendered_lines > 0)) {
            return;
        }

        self.dirty = false;
        let elapsed = self.start.expect("tick noted the start").elapsed();

        self.frame_buf.clear();
        self.frame_buf.push_str(term::HIDE_CURSOR);

        for (line, item) in rows {
            self.frame_buf.push_str(term::CLEAR_LINE);
            let frame = FrameContext {
                spinner_frame: self.spinner_frame,
                spinner_tick: self.spinner_tick,
                elapsed,
                show_elapsed: item.show_elapsed_time() || self.with_elapsed_time,
                spinner_style: item.spinner_style().unwrap_or(self.spinner_style),
                annotation_style: item.annotation_style().unwrap_or(self.annotation_style),
            };
            line.render_to(item, &frame, &mut self.frame_buf);
            self.frame_buf.push('\n');
        }

        // Clear any leftover lines from a previous frame whose rows have since completed.
        let stale = self.rendered_lines.saturating_sub(active_count);

        for _ in 0..stale {
            self.frame_buf.push_str(term::CLEAR_LINE);
            self.frame_buf.push('\n');
        }

        // Move the cursor back up to the first row.
        let total = active_count + stale;

        if total > 0 {
            let _ = write!(self.frame_buf, "\x1b[{total}A");
        }

        self.output.with_lock(|out| {
            let _ = out.write_all(self.frame_buf.as_bytes());
            let _ = out.flush();
        });

        self.rendered_lines = active_count;
    }

    /// Restore the terminal once every row has completed.
    pub(crate) fn finish(&self) {
        let _ = term::reset_on(self.output);
    }
}
