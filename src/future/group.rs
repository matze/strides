//! Multi-line progress display for concurrent futures.

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use futures_lite::Stream;
use owo_colors::Style;

use crate::line::{FrameContext, Line};
use crate::progressive::ProgressiveFuture;
use crate::spinner::{Spinner, Ticks};
use crate::term::{self, clear_line, CursorGuard, Output};
use crate::Theme;

/// One slot in a [`Group`]: the wrapped future and its render line.
struct Slot<'a, O> {
    work: Pin<Box<dyn ProgressiveFuture<'a, Output = O> + 'a>>,
    line: Line<'a>,
}

/// A group of [`ProgressiveFuture`]s rendered as one line per task.
///
/// `Group` owns its work and polls every active slot on each `poll_next`. After polling, it reads
/// each slot's progress via the [`Progressive`](crate::progressive::Progressive) trait and repaints
/// the corresponding line. When a future resolves the line is removed and the output is yielded
/// from the [`Stream`] impl.
///
/// Push futures with [`push`](Group::push). The setters mirrored on
/// [`FutureExt`](crate::future::FutureExt) ([`with_label`](crate::future::FutureExt::with_label),
/// [`with_messages`](crate::future::FutureExt::with_messages),
/// [`with_progress`](crate::future::FutureExt::with_progress),
/// [`with_elapsed_time`](crate::future::FutureExt::with_elapsed_time)) lift a bare future into a
/// [`ProgressFuture`](crate::future::ProgressFuture); use
/// [`progressive()`](crate::future::FutureExt::progressive) to lift explicitly when pushing a
/// future with no configuration.
///
/// Group-wide defaults set via [`with_spinner_style`](Group::with_spinner_style),
/// [`with_annotation_style`](Group::with_annotation_style) and
/// [`with_elapsed_time`](Group::with_elapsed_time) apply to any row that doesn't supply its own.
/// Per-row overrides set via [`ProgressFuture::with_theme`](crate::future::ProgressFuture::with_theme),
/// [`with_spinner_style`](crate::future::ProgressFuture::with_spinner_style),
/// [`with_annotation_style`](crate::future::ProgressFuture::with_annotation_style) and
/// [`with_elapsed_time`](crate::future::ProgressFuture::with_elapsed_time) take precedence on
/// that row.
///
/// ```rust,no_run
/// use std::time::Duration;
/// use futures_lite::{StreamExt, future};
/// use strides::future::{FutureExt, Group};
/// use strides::spinner;
///
/// future::block_on(async {
///     let mut group = Group::new(spinner::styles::DOTS_3);
///     group.push(async_io::Timer::after(Duration::from_secs(1)).with_label("fast"));
///     group.push(async_io::Timer::after(Duration::from_secs(3)).with_label("slow"));
///     group.for_each(|_| {}).await;
/// });
/// ```
pub struct Group<'a, O> {
    slots: Vec<Option<Slot<'a, O>>>,
    buffer: VecDeque<O>,
    theme: Theme<'a>,
    ticks: Ticks<'a>,
    spinner_frame: Option<&'a str>,
    spinner_tick: u64,
    spinner_style: Style,
    annotation_style: Style,
    with_elapsed_time: bool,
    start: Option<Instant>,
    output: Output,
    is_tty: bool,
    rendered_lines: usize,
    dirty: bool,
    _guard: CursorGuard,
}

impl<'a, O> Group<'a, O> {
    /// Create a new group using `theme` as the default for rows that don't supply their own.
    pub fn new(theme: impl Into<Theme<'a>>) -> Self {
        let theme = theme.into();
        let output = theme.output;
        let is_tty = output.is_terminal();

        // A non-terminal output never renders, so active ticks would only wake the task every
        // interval for nothing. Substitute the inactive spinner's never-yielding ticks.
        let ticks = if is_tty {
            theme.spinner.ticks()
        } else {
            Spinner::inactive().ticks()
        };
        Self {
            slots: Vec::new(),
            buffer: VecDeque::new(),
            theme,
            ticks,
            spinner_frame: None,
            spinner_tick: 0,
            spinner_style: Style::new(),
            annotation_style: Style::new(),
            with_elapsed_time: false,
            start: None,
            output,
            is_tty,
            rendered_lines: 0,
            dirty: true,
            _guard: CursorGuard { output, is_tty },
        }
    }

    /// Default spinner style for rows that don't supply their own via
    /// [`ProgressFuture::with_spinner_style`](crate::future::ProgressFuture::with_spinner_style).
    pub fn with_spinner_style(mut self, spinner_style: Style) -> Self {
        self.spinner_style = spinner_style;
        self
    }

    /// Default annotation (label) style for rows that don't supply their own via
    /// [`ProgressFuture::with_annotation_style`](crate::future::ProgressFuture::with_annotation_style).
    pub fn with_annotation_style(mut self, annotation_style: Style) -> Self {
        self.annotation_style = annotation_style;
        self
    }

    /// Default for showing elapsed time. Rows can override by calling
    /// [`with_elapsed_time`](crate::future::ProgressFuture::with_elapsed_time) on the row itself.
    pub fn with_elapsed_time(mut self) -> Self {
        self.with_elapsed_time = true;
        self
    }

    /// Add a future to the group. The future must implement [`ProgressiveFuture`]; calling any of
    /// the [`FutureExt`](crate::future::FutureExt) setters
    /// ([`with_label`](crate::future::FutureExt::with_label),
    /// [`with_messages`](crate::future::FutureExt::with_messages),
    /// [`with_progress`](crate::future::FutureExt::with_progress),
    /// [`with_elapsed_time`](crate::future::FutureExt::with_elapsed_time)) on a bare future
    /// produces one. For a bare future with no configuration, call
    /// [`progressive()`](crate::future::FutureExt::progressive) to lift it explicitly.
    pub fn push<F>(&mut self, mut fut: F)
    where
        F: ProgressiveFuture<'a, Output = O> + 'a,
    {
        let line = match fut.theme() {
            Some(row_theme) => Line::new(row_theme),
            None => Line::new(&self.theme),
        };
        fut.detach_rendering();
        self.slots.push(Some(Slot {
            work: Box::pin(fut),
            line,
        }));
        self.dirty = true;
    }
}

impl<O> Stream for Group<'_, O>
where
    O: Unpin,
{
    type Item = O;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if this.slots.is_empty() {
            return Poll::Ready(None);
        }

        this.start.get_or_insert_with(Instant::now);

        if let Poll::Ready(frame) = Pin::new(&mut this.ticks).poll_next(cx) {
            this.spinner_frame = frame;
            this.spinner_tick = this.spinner_tick.wrapping_add(1);
            this.dirty = true;
        }

        // Poll every active slot; buffer every completion so simultaneous Readys aren't lost.
        for slot in this.slots.iter_mut() {
            if let Some(s) = slot {
                if let Poll::Ready(out) = s.work.as_mut().poll(cx) {
                    this.buffer.push_back(out);
                    *slot = None;
                    this.dirty = true;
                }
            }
        }

        let active_count = this.slots.iter().filter(|s| s.is_some()).count();

        if this.is_tty && this.dirty && (active_count > 0 || this.rendered_lines > 0) {
            this.dirty = false;
            let elapsed = this.start.expect("start initialised above").elapsed();

            this.output.with_lock(|stdout| {
                let _ = stdout.write_all(term::HIDE_CURSOR);

                for slot in this.slots.iter_mut().flatten() {
                    let _ = clear_line(stdout);
                    let item = slot.work.as_ref().get_ref();
                    let frame = FrameContext {
                        spinner_frame: this.spinner_frame,
                        spinner_tick: this.spinner_tick,
                        elapsed,
                        show_elapsed: item.show_elapsed_time() || this.with_elapsed_time,
                        spinner_style: item.spinner_style().unwrap_or(this.spinner_style),
                        annotation_style: item.annotation_style().unwrap_or(this.annotation_style),
                    };
                    let rendered = slot.line.render_into(item, &frame);
                    let _ = stdout.write_all(rendered.as_bytes());
                    let _ = stdout.write_all(b"\n");
                }

                // Clear any leftover lines from a previous frame whose slots have since completed.
                let stale = this.rendered_lines.saturating_sub(active_count);
                for _ in 0..stale {
                    let _ = clear_line(stdout);
                    let _ = stdout.write_all(b"\n");
                }

                let total = active_count + stale;
                if total > 0 {
                    let _ = term::move_up(stdout, total as u16);
                }

                let _ = stdout.flush();
            });
            this.rendered_lines = active_count;
        }

        if let Some(out) = this.buffer.pop_front() {
            return Poll::Ready(Some(out));
        }

        if active_count == 0 {
            // All slots done and emitted; one final return.
            let _ = term::reset_on(this.output);
            return Poll::Ready(None);
        }

        Poll::Pending
    }
}
