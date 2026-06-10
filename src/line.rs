//! Per-row rendering helper.
//!
//! A [`Line`] holds the theme bits used to render one terminal row (bar, bar width, layout) and a
//! reusable buffer. It only fills the buffer — callers (standalone wrappers, [`Group`]s) own the
//! actual stdout writes and cursor management.

use std::time::Duration;

use owo_colors::Style;

use crate::bar::Bar;
use crate::layout::{Layout, RenderContext};
use crate::progressive::Progressive;
use crate::term::{self, clear_line, Output};
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

pub(crate) struct Line {
    bar: Option<Bar>,
    bar_width: usize,
    layout: Layout,
    render_buf: String,
}

impl Line {
    pub(crate) fn new(theme: &Theme) -> Self {
        Self {
            bar: theme.bar.clone(),
            bar_width: theme.effective_bar_width(),
            layout: theme.layout.clone(),
            render_buf: String::new(),
        }
    }

    /// Render `item` together with `frame` into this line's internal buffer and return a borrowed
    /// view of the rendered bytes. The buffer is cleared before each render.
    pub(crate) fn render_into<P: Progressive + ?Sized>(
        &mut self,
        item: &P,
        frame: &FrameContext,
    ) -> &str {
        let ctx = RenderContext {
            spinner: frame.spinner_frame,
            spinner_tick: frame.spinner_tick,
            elapsed: frame.elapsed,
            show_elapsed: frame.show_elapsed,
            bar: self.bar.as_ref(),
            bar_width: self.bar_width,
            progress: item.progress(),
            bytes_done: item.bytes_done(),
            bytes_total: item.bytes_total(),
            rate: item.rate(),
            label: item.label(),
            message: item.message(),
            spinner_style: frame.spinner_style,
            annotation_style: frame.annotation_style,
        };
        self.render_buf.clear();
        self.layout.render(&ctx, &mut self.render_buf);
        &self.render_buf
    }

    /// Render `item` and write the result to `output` as the single line of a standalone wrapper:
    /// hide cursor, clear current line, write content, flush. No-op when `is_tty` is false.
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
        let _ = self.render_into(item, frame);
        output.with_lock(|w| {
            let _ = w.write_all(term::HIDE_CURSOR);
            let _ = clear_line(w);
            let _ = w.write_all(self.render_buf.as_bytes());
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
            let _ = clear_line(w);
            let _ = w.flush();
        });
    }
}
