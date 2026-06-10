//! Multi-line progress display for concurrent streams.

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_lite::Stream;
use owo_colors::Style;

use crate::group::GroupCore;
use crate::line::Line;
use crate::progressive::ProgressiveStream;
use crate::Theme;

/// One slot in a [`Group`]: the wrapped stream and its render line.
struct Slot<'a, I> {
    work: Pin<Box<dyn ProgressiveStream<Item = I> + 'a>>,
    line: Line,
}

/// A group of [`ProgressiveStream`]s rendered as one line per stream.
///
/// Each [`poll_next`](Stream::poll_next) call polls every active slot once. Items yielded by the
/// inner streams are queued and drained one-per-call from the [`Stream`] impl. When an inner
/// stream returns `Ready(None)` its line is removed. The Group itself returns `Ready(None)` once
/// every slot has terminated.
///
/// Group-wide defaults set via [`with_spinner_style`](Group::with_spinner_style),
/// [`with_annotation_style`](Group::with_annotation_style) and
/// [`with_elapsed_time`](Group::with_elapsed_time) apply to any row that doesn't supply its own.
/// Per-row overrides via the matching setters on
/// [`ProgressStream`](crate::stream::ProgressStream) /
/// [`ProgressBytesStream`](crate::stream::ProgressBytesStream) take precedence on that row.
pub struct Group<'a, I> {
    slots: Vec<Option<Slot<'a, I>>>,
    buffer: VecDeque<I>,
    core: GroupCore,
}

impl<'a, I> Group<'a, I> {
    /// Create a new group using `theme` as the default for rows that don't supply their own.
    pub fn new(theme: impl Into<Theme>) -> Self {
        Self {
            slots: Vec::new(),
            buffer: VecDeque::new(),
            core: GroupCore::new(theme.into()),
        }
    }

    /// Default spinner style for rows that don't supply their own.
    pub fn with_spinner_style(mut self, spinner_style: Style) -> Self {
        self.core.spinner_style = spinner_style;
        self
    }

    /// Default annotation (label) style for rows that don't supply their own.
    pub fn with_annotation_style(mut self, annotation_style: Style) -> Self {
        self.core.annotation_style = annotation_style;
        self
    }

    /// Default for showing elapsed time on rows that don't supply their own.
    pub fn with_elapsed_time(mut self) -> Self {
        self.core.with_elapsed_time = true;
        self
    }

    /// Add a stream to the group. The stream must implement [`ProgressiveStream`]; use
    /// [`progressive`](crate::stream::StreamExt::progressive) or
    /// [`progressive_bytes`](crate::stream::StreamExt::progressive_bytes) on a bare stream to
    /// obtain one.
    pub fn push<S>(&mut self, mut stream: S)
    where
        S: ProgressiveStream<Item = I> + 'a,
    {
        let line = match stream.theme() {
            Some(row_theme) => Line::new(row_theme),
            None => Line::new(&self.core.theme),
        };
        stream.detach_rendering();
        self.slots.push(Some(Slot {
            work: Box::pin(stream),
            line,
        }));
        self.core.mark_dirty();
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

        this.core.tick(cx);

        // Poll each active slot once, collecting any newly-yielded items into the buffer.
        for slot in this.slots.iter_mut() {
            if let Some(s) = slot {
                match s.work.as_mut().poll_next(cx) {
                    Poll::Ready(Some(item)) => {
                        this.buffer.push_back(item);
                        this.core.mark_dirty();
                    }
                    Poll::Ready(None) => {
                        *slot = None;
                        this.core.mark_dirty();
                    }
                    Poll::Pending => {}
                }
            }
        }

        let active_count = this.slots.iter().filter(|s| s.is_some()).count();

        this.core.repaint(
            active_count,
            this.slots
                .iter()
                .flatten()
                .map(|s| (&s.line, s.work.as_ref().get_ref())),
        );

        if let Some(item) = this.buffer.pop_front() {
            return Poll::Ready(Some(item));
        }

        if active_count == 0 {
            this.core.finish();
            return Poll::Ready(None);
        }

        Poll::Pending
    }
}
