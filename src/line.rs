//! Per-row rendering helper.
//!
//! A [`Line`] holds the theme bits used to render one terminal row (bar, bar width, layout). It
//! only fills buffers — callers (standalone wrappers, [`Group`]s) own the actual terminal writes
//! and cursor management.

use std::time::Duration;

use owo_colors::Style;

use crate::bar::Bar;
use crate::layout::{Layout, RenderContext};
use crate::progressive::Progressive;
use crate::term::{self, Output};
use crate::Theme;

/// Per-frame rendering inputs that are not part of [`Progressive`]. The spinner frame, the
/// elapsed time, and style overrides.
pub(crate) struct FrameContext {
    pub spinner_frame: Option<&'static str>,
    pub spinner_tick: u64,
    pub elapsed: Duration,
    pub show_elapsed: bool,
    pub spinner_style: Style,
    pub annotation_style: Style,
}

/// Assemble the [`RenderContext`] for one frame of `item`.
fn context<'a, P: Progressive + ?Sized>(
    bar: Option<&'a Bar>,
    bar_width: usize,
    item: &'a P,
    frame: &FrameContext,
) -> RenderContext<'a> {
    RenderContext {
        spinner: frame.spinner_frame,
        spinner_tick: frame.spinner_tick,
        elapsed: frame.elapsed,
        show_elapsed: frame.show_elapsed,
        bar,
        bar_width,
        progress: item.progress(),
        bytes_done: item.bytes_done(),
        bytes_total: item.bytes_total(),
        rate: item.rate(),
        label: item.label(),
        message: item.message(),
        spinner_style: frame.spinner_style,
        annotation_style: frame.annotation_style,
    }
}

pub(crate) struct Line {
    bar: Option<Bar>,
    bar_width: usize,
    layout: Layout,
    /// Frame buffer for the standalone path, reused across frames.
    frame_buf: String,
}

impl Line {
    pub(crate) fn new(theme: &Theme) -> Self {
        Self {
            bar: theme.bar.clone(),
            bar_width: theme.effective_bar_width(),
            layout: theme.layout.clone(),
            frame_buf: String::new(),
        }
    }

    /// Render `item` together with `frame`, appending to `buf`. Used by
    /// [`Group`](crate::future::Group)s, which assemble many rows into one frame buffer.
    pub(crate) fn render_to<P: Progressive + ?Sized>(
        &self,
        item: &P,
        frame: &FrameContext,
        buf: &mut String,
    ) {
        let ctx = context(self.bar.as_ref(), self.bar_width, item, frame);
        self.layout.render(&ctx, buf);
    }

    /// Render `item` and write the result to `output` as the single line of a standalone wrapper.
    /// The whole frame — hide cursor, clear line, content — is assembled into one reusable buffer
    /// and written in a single call, then flushed. No-op when `is_tty` is false.
    pub(crate) fn standalone_render<P: Progressive + ?Sized>(
        &mut self,
        item: &P,
        frame: &FrameContext,
        output: Output,
        is_tty: bool,
    ) {
        if !is_tty {
            return;
        }

        let mut buf = std::mem::take(&mut self.frame_buf);
        buf.clear();
        buf.push_str(term::HIDE_CURSOR);
        buf.push_str(term::CLEAR_LINE);
        self.render_to(item, frame, &mut buf);
        self.frame_buf = buf;

        output.with_lock(|w| {
            let _ = w.write_all(self.frame_buf.as_bytes());
            let _ = w.flush();
        });
    }

    /// Clear the current line. Used by a standalone wrapper when its work completes; cursor
    /// restoration happens via the wrapper's [`CursorGuard`](crate::term::CursorGuard).
    pub(crate) fn standalone_clear(output: Output, is_tty: bool) {
        if !is_tty {
            return;
        }

        output.with_lock(|w| {
            let _ = w.write_all(term::CLEAR_LINE.as_bytes());
            let _ = w.flush();
        });
    }
}
