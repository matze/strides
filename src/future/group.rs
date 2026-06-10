//! Multi-line progress display for concurrent futures.

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_lite::Stream;
use owo_colors::Style;

use crate::group::GroupCore;
use crate::line::Line;
use crate::progressive::ProgressiveFuture;
use crate::Theme;

/// One slot in a [`Group`]: the wrapped future and its render line.
struct Slot<'a, O> {
    work: Pin<Box<dyn ProgressiveFuture<Output = O> + 'a>>,
    line: Line,
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
    core: GroupCore,
}

impl<'a, O> Group<'a, O> {
    /// Create a new group using `theme` as the default for rows that don't supply their own.
    pub fn new(theme: impl Into<Theme>) -> Self {
        Self {
            slots: Vec::new(),
            buffer: VecDeque::new(),
            core: GroupCore::new(theme.into()),
        }
    }

    /// Default spinner style for rows that don't supply their own via
    /// [`ProgressFuture::with_spinner_style`](crate::future::ProgressFuture::with_spinner_style).
    pub fn with_spinner_style(mut self, spinner_style: Style) -> Self {
        self.core.spinner_style = spinner_style;
        self
    }

    /// Default annotation (label) style for rows that don't supply their own via
    /// [`ProgressFuture::with_annotation_style`](crate::future::ProgressFuture::with_annotation_style).
    pub fn with_annotation_style(mut self, annotation_style: Style) -> Self {
        self.core.annotation_style = annotation_style;
        self
    }

    /// Default for showing elapsed time. Rows can override by calling
    /// [`with_elapsed_time`](crate::future::ProgressFuture::with_elapsed_time) on the row itself.
    pub fn with_elapsed_time(mut self) -> Self {
        self.core.with_elapsed_time = true;
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
        F: ProgressiveFuture<Output = O> + 'a,
    {
        let line = match fut.theme() {
            Some(row_theme) => Line::new(row_theme),
            None => Line::new(&self.core.theme),
        };
        fut.detach_rendering();
        self.slots.push(Some(Slot {
            work: Box::pin(fut),
            line,
        }));
        self.core.mark_dirty();
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

        this.core.tick(cx);

        // Poll every active slot; buffer every completion so simultaneous Readys aren't lost.
        for slot in this.slots.iter_mut() {
            if let Some(s) = slot {
                if let Poll::Ready(out) = s.work.as_mut().poll(cx) {
                    this.buffer.push_back(out);
                    *slot = None;
                    this.core.mark_dirty();
                }
            }
        }

        let active_count = this.slots.iter().filter(|s| s.is_some()).count();

        this.core.repaint(
            active_count,
            this.slots
                .iter_mut()
                .flatten()
                .map(|s| (&mut s.line, s.work.as_ref().get_ref())),
        );

        if let Some(out) = this.buffer.pop_front() {
            return Poll::Ready(Some(out));
        }

        if active_count == 0 {
            // All slots done and emitted; one final return.
            this.core.finish();
            return Poll::Ready(None);
        }

        Poll::Pending
    }
}
