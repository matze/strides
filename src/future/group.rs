//! Multi-line progress display for concurrent futures.

use std::collections::VecDeque;
use std::io::{IsTerminal, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use futures_lite::Stream;
use owo_colors::Style;

use crate::line::{FrameContext, Line};
use crate::progressive::ProgressiveFuture;
use crate::spinner::Ticks;
use crate::term::{self, clear_line, CursorGuard};
use crate::Theme;

/// One slot in a [`Group`]: the wrapped future and its render line.
struct Slot<'a, O> {
    work: Pin<Box<dyn ProgressiveFuture<Output = O> + 'a>>,
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
/// tracked-only [`ProgressFuture`](crate::future::ProgressFuture); use
/// [`progressive()`](crate::future::FutureExt::progressive) to lift explicitly when pushing a
/// future with no configuration.
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
    spinner_char: Option<char>,
    spinner_style: Style,
    annotation_style: Style,
    with_elapsed_time: bool,
    start: Option<Instant>,
    is_tty: bool,
    rendered_lines: usize,
    dirty: bool,
    _guard: CursorGuard,
}

impl<'a, O> Group<'a, O> {
    /// Create a new group using `theme` to style every line.
    pub fn new(theme: impl Into<Theme<'a>>) -> Self {
        let theme = theme.into();
        let is_tty = std::io::stdout().is_terminal();
        let ticks = theme.spinner.ticks();
        Self {
            slots: Vec::new(),
            buffer: VecDeque::new(),
            theme,
            ticks,
            spinner_char: None,
            spinner_style: Style::new(),
            annotation_style: Style::new(),
            with_elapsed_time: false,
            start: None,
            is_tty,
            rendered_lines: 0,
            dirty: true,
            _guard: CursorGuard { is_tty },
        }
    }

    /// Apply `spinner_style` to the spinner character on every line.
    pub fn with_spinner_style(mut self, spinner_style: Style) -> Self {
        self.spinner_style = spinner_style;
        self
    }

    /// Apply `annotation_style` to every line's label.
    pub fn with_annotation_style(mut self, annotation_style: Style) -> Self {
        self.annotation_style = annotation_style;
        self
    }

    /// Prepend `[Xs]` (seconds since the group was first polled) to each line.
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
    pub fn push<F>(&mut self, fut: F)
    where
        F: ProgressiveFuture<Output = O> + 'a,
    {
        let line = Line::new(&self.theme);
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

        if let Poll::Ready(ch) = Pin::new(&mut this.ticks).poll_next(cx) {
            this.spinner_char = ch;
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
            let frame = FrameContext {
                spinner_char: this.spinner_char,
                elapsed,
                show_elapsed: this.with_elapsed_time,
                spinner_style: this.spinner_style,
                annotation_style: this.annotation_style,
            };

            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(term::HIDE_CURSOR);

            for slot in this.slots.iter_mut().flatten() {
                let _ = clear_line(&mut stdout);
                let item = slot.work.as_ref().get_ref();
                let rendered = slot.line.render_into(item, &frame);
                let _ = stdout.write_all(rendered.as_bytes());
                let _ = stdout.write_all(b"\n");
            }

            // Clear any leftover lines from a previous frame whose slots have since completed.
            let stale = this.rendered_lines.saturating_sub(active_count);
            for _ in 0..stale {
                let _ = clear_line(&mut stdout);
                let _ = stdout.write_all(b"\n");
            }

            let total = active_count + stale;
            if total > 0 {
                let _ = term::move_up(&mut stdout, total as u16);
            }

            let _ = stdout.flush();
            this.rendered_lines = active_count;
        }

        if let Some(out) = this.buffer.pop_front() {
            return Poll::Ready(Some(out));
        }

        if active_count == 0 {
            // All slots done and emitted; one final return.
            let _ = term::reset();
            return Poll::Ready(None);
        }

        Poll::Pending
    }
}
