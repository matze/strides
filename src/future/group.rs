use std::fmt::Display;
use std::future::Future;
use std::task::{Context, Poll};
use std::time::Instant;
use std::{io::Write, pin::Pin};

use crossterm::{QueueableCommand, cursor};
use futures_lite::{FutureExt as _, Stream, StreamExt as _};
use futures_util::stream::FuturesUnordered;
use owo_colors::OwoColorize;

use crate::Theme;
use crate::bar::Bar;
use crate::spinner::Ticks;
use crate::term::{clear_line, reset};

/// Helper future that allows us to track the completion status of the wrapped future F.
struct Annotated<F> {
    inner: F,
    id: usize,
}

impl<F> Annotated<F>
where
    F: Future,
{
    fn new(inner: F, id: usize) -> Self {
        Self { inner, id }
    }
}

impl<F> Future for Annotated<F>
where
    F: Future + Unpin,
{
    type Output = (F::Output, usize);

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        let id = this.id;

        match this.inner.poll(cx) {
            Poll::Ready(output) => Poll::Ready((output, id)),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// A future together with its display configuration in a [`Group`].
///
/// `Task` is constructed from a bare future via `Into<Task<...>>` and configured fluently with
/// [`with_label`](Task::with_label), [`with_messages`](Task::with_messages) and
/// [`with_progress`](Task::with_progress). The same setters are mirrored on
/// [`FutureExt`](super::FutureExt), so a future can be lifted into a `Task` simply by calling one
/// of them.
///
/// ```rust,no_run
/// use strides::future::FutureExt;
///
/// # async fn fetch() {}
/// let task = fetch().with_label("alpha");
/// ```
pub struct Task<'a, F> {
    fut: F,
    label: Option<String>,
    messages: Option<Box<dyn Stream<Item = String> + Unpin + 'a>>,
    progress: Option<Box<dyn Stream<Item = f64> + Unpin + 'a>>,
}

impl<'a, F> From<F> for Task<'a, F>
where
    F: Future,
{
    fn from(fut: F) -> Self {
        Self {
            fut,
            label: None,
            messages: None,
            progress: None,
        }
    }
}

impl<'a, F> Task<'a, F> {
    /// Attach a static `label` to this task, rendered with the group's annotation style
    /// between the elapsed-time block and the progress bar.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Replace the displayed message each time `messages` yields a value.
    ///
    /// When the stream is exhausted the last value remains visible.
    pub fn with_messages<S, D>(mut self, messages: S) -> Self
    where
        S: Stream<Item = D> + Unpin + 'a,
        D: Display + 'a,
    {
        self.messages = Some(Box::new(messages.map(|d| d.to_string())));
        self
    }

    /// Drive a per-task progress bar from a stream of fractions in `0.0..=1.0`.
    ///
    /// The latest value wins. The bar's style and width are taken from the [`Theme`] passed to
    /// [`Group::new`].
    pub fn with_progress<S>(mut self, progress: S) -> Self
    where
        S: Stream<Item = f64> + Unpin + 'a,
    {
        self.progress = Some(Box::new(progress));
        self
    }
}

/// Per-task state tracking label, current message and current progress fraction along with the
/// streams driving them.
struct TaskState<'a> {
    label: Option<String>,
    message: Option<String>,
    messages: Option<Box<dyn Stream<Item = String> + Unpin + 'a>>,
    progress: Option<f64>,
    progress_stream: Option<Box<dyn Stream<Item = f64> + Unpin + 'a>>,
}

/// A group of futures displayed as multi-line progress with per-task annotations.
///
/// Each future in the group occupies its own terminal line showing a spinner, optional progress bar
/// and a message. Lines are removed as futures complete.
///
/// Tasks are configured fluently and handed to [`push()`](Group::push), either as a bare future
/// (via `Into<Task<...>>`) or as a [`Task`] configured with [`with_label`](Task::with_label),
/// [`with_messages`](Task::with_messages) and [`with_progress`](Task::with_progress).
///
/// `Group` implements [`Stream`]. Each time a future completes, the stream yields its output.
///
/// # Example
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
pub struct Group<'a, F> {
    /// Group of futures.
    inner: FuturesUnordered<Annotated<F>>,
    /// Spinner tick stream.
    ticks: Ticks<'a>,
    /// Per-task state. Set to `None` when the corresponding future completes.
    tasks: Vec<Option<TaskState<'a>>>,
    /// Annotation style.
    annotation_style: owo_colors::Style,
    /// Current spinner character.
    spinner: Option<char>,
    /// Spinner style.
    spinner_style: owo_colors::Style,
    /// Bar style used for tasks configured with [`Task::with_progress`].
    bar: Bar<'a>,
    /// Width of the progress bar in characters.
    bar_width: usize,
    /// `true` if elapsed time should be shown for each future.
    with_elapsed_time: bool,
    /// Time when the stream was first awaited.
    start: Option<Instant>,
    /// Whether the display needs to be redrawn.
    dirty: bool,
    /// Number of lines printed by the previous render, used to clear leftovers when the
    /// active count shrinks.
    rendered_lines: usize,
}

impl<'a, F> Group<'a, F>
where
    F: Future,
{
    /// Create a new group with the given theme.
    ///
    /// Accepts a [`Theme`] or a bare [`Spinner`](crate::spinner::Spinner) (converted via `Into`).
    /// When the theme includes a [`Bar`] it is used to render per-task progress for tasks
    /// configured via [`Task::with_progress`].
    pub fn new<S: Into<Theme<'a>>>(theme: S) -> Self {
        let theme = theme.into();
        let bar_width = theme.effective_bar_width();

        Self {
            inner: FuturesUnordered::new(),
            ticks: theme.spinner.ticks(),
            tasks: Vec::new(),
            annotation_style: owo_colors::Style::new(),
            spinner: None,
            spinner_style: owo_colors::Style::new(),
            bar: theme.bar,
            bar_width,
            with_elapsed_time: false,
            start: None,
            dirty: true,
            rendered_lines: 0,
        }
    }

    /// Apply `spinner_style` to the spinner character on every line.
    pub fn with_spinner_style(mut self, spinner_style: owo_colors::Style) -> Self {
        self.spinner_style = spinner_style;
        self
    }

    /// Apply `annotation_style` to each task's prefix.
    pub fn with_annotation_style(mut self, annotation_style: owo_colors::Style) -> Self {
        self.annotation_style = annotation_style;
        self
    }

    /// When `true`, prepend `[Xs]` (seconds since the group was first polled) to each line.
    pub fn with_elapsed_time(mut self, with_elapsed_time: bool) -> Self {
        self.with_elapsed_time = with_elapsed_time;
        self
    }

    /// Add a task to the group.
    ///
    /// Accepts either a bare future (lifted into a [`Task`] via `From`) or a [`Task`] configured
    /// with [`with_label`](Task::with_label), [`with_messages`](Task::with_messages) and/or
    /// [`with_progress`](Task::with_progress).
    pub fn push(&mut self, task: impl Into<Task<'a, F>>) {
        let task = task.into();
        let id = self.tasks.len();
        self.tasks.push(Some(TaskState {
            label: task.label,
            message: None,
            messages: task.messages,
            progress: None,
            progress_stream: task.progress,
        }));
        self.inner.push(Annotated::new(task.fut, id));
    }
}

impl<F> Stream for Group<'_, F>
where
    F: Future + Unpin,
{
    type Item = F::Output;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let inner = Pin::new(&mut this.inner);
        let ticks = Pin::new(&mut this.ticks);
        let elapsed = this.start.get_or_insert_with(Instant::now).elapsed();

        // Poll the spinner stream.
        if let Poll::Ready(spinner) = ticks.poll_next(cx) {
            this.spinner = spinner;
            this.dirty = true;
        }

        // Poll per-task message and progress streams. Drain the progress stream so the bar always
        // reflects the latest value rather than lagging behind queued updates.
        for task in this.tasks.iter_mut().flatten() {
            if let Some(messages) = task.messages.as_mut()
                && let Poll::Ready(Some(msg)) = Pin::new(messages).poll_next(cx)
            {
                task.message = Some(msg);
                this.dirty = true;
            }

            if let Some(progress_stream) = task.progress_stream.as_mut() {
                while let Poll::Ready(Some(p)) = Pin::new(&mut *progress_stream).poll_next(cx) {
                    task.progress = Some(p.clamp(0.0, 1.0));
                    this.dirty = true;
                }
            }
        }

        let item = match inner.poll_next(cx) {
            Poll::Ready(Some((output, id))) => {
                this.tasks[id] = None;
                this.dirty = true;
                Poll::Ready(Some(output))
            }
            Poll::Ready(None) => {
                let _ = reset();
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        };

        if this.dirty && !matches!(item, Poll::Ready(None)) {
            this.dirty = false;

            let mut stdout = std::io::stdout();
            let _ = stdout.queue(cursor::Hide);

            let active_count = this.tasks.iter().filter(|t| t.is_some()).count();

            for task in this.tasks.iter().flatten() {
                let _ = clear_line(&mut stdout);

                if let Some(spinner) = &this.spinner {
                    print!("{} ", spinner.style(this.spinner_style));
                }

                if this.with_elapsed_time {
                    print!("[{:.2}s] ", elapsed.as_secs_f64());
                }

                if let Some(label) = &task.label {
                    print!("{} ", label.style(this.annotation_style));
                }

                if let Some(progress) = task.progress {
                    let bar = this.bar.render(this.bar_width, progress);

                    if !bar.is_empty() {
                        print!("{bar} ");
                    }
                }

                if let Some(message) = &task.message {
                    println!("{message}");
                } else {
                    println!();
                }
            }

            // The previous render may have drawn more lines than we just did. Clear those leftovers
            // so completed tasks do not linger as stray output.
            let stale_lines = this.rendered_lines.saturating_sub(active_count);

            for _ in 0..stale_lines {
                let _ = clear_line(&mut stdout);
                println!();
            }

            this.rendered_lines = active_count;

            let total_advanced = active_count + stale_lines;

            if total_advanced > 0 {
                let _ = stdout.queue(cursor::MoveUp(total_advanced as u16));
            }

            let _ = stdout.flush();
        }

        item
    }
}
