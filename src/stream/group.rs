//! Multi-line progress display for concurrent streams.

use std::collections::VecDeque;
use std::io::{IsTerminal, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use futures_lite::Stream;
use owo_colors::Style;

use crate::line::{FrameContext, Line};
use crate::progressive::ProgressiveStream;
use crate::spinner::Ticks;
use crate::term::{self, clear_line, CursorGuard};
use crate::Theme;

/// One slot in a [`Group`]: the wrapped stream and its render line.
struct Slot<'a, I> {
    work: Pin<Box<dyn ProgressiveStream<Item = I> + 'a>>,
    line: Line<'a>,
}

/// A group of [`ProgressiveStream`]s rendered as one line per stream.
///
/// Each [`poll_next`](Stream::poll_next) call polls every active slot once. Items yielded by the
/// inner streams are queued and drained one-per-call from the [`Stream`] impl. When an inner
/// stream returns `Ready(None)` its line is removed. The Group itself returns `Ready(None)` once
/// every slot has terminated.
pub struct Group<'a, I> {
    slots: Vec<Option<Slot<'a, I>>>,
    buffer: VecDeque<I>,
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

impl<'a, I> Group<'a, I> {
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

    /// Add a stream to the group. The stream must implement [`ProgressiveStream`]; use
    /// [`progressive`](crate::stream::StreamExt::progressive) or
    /// [`progressive_bytes`](crate::stream::StreamExt::progressive_bytes) on a bare stream to
    /// obtain one.
    pub fn push<S>(&mut self, stream: S)
    where
        S: ProgressiveStream<Item = I> + 'a,
    {
        let line = Line::new(&self.theme);
        self.slots.push(Some(Slot {
            work: Box::pin(stream),
            line,
        }));
        self.dirty = true;
    }
}

impl<I> Stream for Group<'_, I>
where
    I: Unpin,
{
    type Item = I;

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

        // Poll each active slot once, collecting any newly-yielded items into the buffer.
        for slot in this.slots.iter_mut() {
            if let Some(s) = slot {
                match s.work.as_mut().poll_next(cx) {
                    Poll::Ready(Some(item)) => {
                        this.buffer.push_back(item);
                        this.dirty = true;
                    }
                    Poll::Ready(None) => {
                        *slot = None;
                        this.dirty = true;
                    }
                    Poll::Pending => {}
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

        if let Some(item) = this.buffer.pop_front() {
            return Poll::Ready(Some(item));
        }

        if active_count == 0 {
            let _ = term::reset();
            return Poll::Ready(None);
        }

        Poll::Pending
    }
}
