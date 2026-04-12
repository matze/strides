use std::future::Future;
use std::task::{Context, Poll};
use std::time::Instant;
use std::{io::Write, pin::Pin};

use crossterm::{QueueableCommand, cursor, terminal};
use futures::stream::FuturesUnordered;
use futures_lite::{FutureExt as _, Stream, stream};
use owo_colors::OwoColorize;

use crate::spinner::{Spinner, Ticks};
use crate::term::clear_line;

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

/// Per-task state tracking prefix, current message, and an optional messages stream.
struct Task<'a> {
    /// Static prefix/label shown before the message.
    prefix: String,
    /// Current message text, updated by the messages stream.
    message: Option<String>,
    /// Stream of dynamic messages.
    messages: Box<dyn Stream<Item = String> + Unpin + 'a>,
}

/// A group of futures displayed as multi-line progress with per-task annotations.
///
/// Each future in the group occupies its own terminal line showing a spinner and a message. Lines
/// are removed as futures complete.
///
/// Use [`push()`](Group::push) for static annotations or
/// [`push_with_messages()`](Group::push_with_messages) for messages that update dynamically while
/// the future runs.
///
/// `Group` implements [`Stream`], each time a future completes, the stream yields its output.
///
/// # Example
///
/// ```rust,no_run
/// use std::time::Duration;
/// use futures_lite::{StreamExt, future};
/// use strides::future::Group;
/// use strides::spinner;
///
/// future::block_on(async {
///     let mut group = Group::new(spinner::styles::DOTS_3);
///     group.push(async_io::Timer::after(Duration::from_secs(1)), "fast".into());
///     group.push(async_io::Timer::after(Duration::from_secs(3)), "slow".into());
///     group.for_each(|_| {}).await;
/// });
/// ```
pub struct Group<'a, F> {
    /// Group of futures.
    inner: FuturesUnordered<Annotated<F>>,
    /// Spinner tick stream.
    ticks: Ticks<'a>,
    /// Per-task state. Set to `None` when the corresponding future completes.
    tasks: Vec<Option<Task<'a>>>,
    /// Annotation style.
    annotation_style: owo_colors::Style,
    /// Current spinner character.
    spinner: Option<char>,
    /// Spinner style.
    spinner_style: owo_colors::Style,
    /// `true` if elapsed time should be shown for each future.
    with_elapsed_time: bool,
    /// Time when the stream was first awaited.
    start: Option<Instant>,
    /// Whether the display needs to be redrawn.
    dirty: bool,
}

impl<'a, F> Group<'a, F>
where
    F: Future,
{
    pub fn new(spinner: Spinner<'a>) -> Self {
        Self {
            inner: FuturesUnordered::new(),
            ticks: spinner.ticks(),
            tasks: Vec::new(),
            annotation_style: owo_colors::Style::new(),
            spinner: None,
            spinner_style: owo_colors::Style::new(),
            with_elapsed_time: false,
            start: None,
            dirty: true,
        }
    }

    pub fn with_spinner_style(mut self, spinner_style: owo_colors::Style) -> Self {
        self.spinner_style = spinner_style;
        self
    }

    pub fn with_annotation_style(mut self, annotation_style: owo_colors::Style) -> Self {
        self.annotation_style = annotation_style;
        self
    }

    pub fn with_elapsed_time(mut self, with_elapsed_time: bool) -> Self {
        self.with_elapsed_time = with_elapsed_time;
        self
    }

    /// Add `fut` to the group with a static annotation.
    pub fn push(&mut self, fut: F, annotation: String) {
        let id = self.tasks.len();
        self.tasks.push(Some(Task {
            prefix: annotation,
            message: None,
            messages: Box::new(stream::pending()),
        }));
        self.inner.push(Annotated::new(fut, id));
    }

    /// Add `fut` to the group with a static prefix and a stream of dynamic messages.
    ///
    /// The `prefix` is always shown (e.g. `"[1/4]"`).  Each time `messages` yields a value it
    /// replaces the text shown after the prefix.  When the stream is exhausted the last message
    /// remains visible.
    pub fn push_with_messages(
        &mut self,
        fut: F,
        prefix: String,
        messages: impl Stream<Item = String> + Unpin + 'a,
    ) {
        let id = self.tasks.len();
        self.tasks.push(Some(Task {
            prefix,
            message: None,
            messages: Box::new(messages),
        }));
        self.inner.push(Annotated::new(fut, id));
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
        let elapsed = this.start.get_or_insert_with(|| Instant::now()).elapsed();

        // Poll the spinner stream.
        if let Poll::Ready(spinner) = ticks.poll_next(cx) {
            this.spinner = spinner;
            this.dirty = true;
        }

        // Poll per-task message streams.
        for task in this.tasks.iter_mut().flatten() {
            if let Poll::Ready(Some(msg)) = Pin::new(&mut task.messages).poll_next(cx) {
                task.message = Some(msg);
                this.dirty = true;
            }
        }

        let item = match inner.poll_next(cx) {
            Poll::Ready(Some((output, id))) => {
                this.tasks[id] = None;
                this.dirty = true;
                Poll::Ready(Some(output))
            }
            Poll::Ready(None) => {
                let mut stdout = std::io::stdout();
                let _ = reset(&mut stdout);
                let _ = stdout.flush();
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        };

        if this.dirty && !matches!(item, Poll::Ready(None)) {
            this.dirty = false;

            let mut stdout = std::io::stdout();
            let _ = stdout.queue(cursor::Hide);

            for task in &this.tasks {
                let _ = clear_line(&mut stdout);

                if let Some(spinner) = &this.spinner {
                    print!("{} ", spinner.style(this.spinner_style));
                }

                if this.with_elapsed_time {
                    print!("[{:.2}s] ", elapsed.as_secs_f64());
                }

                if let Some(task) = task {
                    let prefix = task.prefix.style(this.annotation_style);

                    if let Some(message) = &task.message {
                        println!("{prefix} {message}");
                    } else {
                        println!("{prefix}");
                    }
                }
            }

            if matches!(item, Poll::Ready(Some(_))) {
                let _ = remove_last_line(&mut stdout);
            }

            // Go up by number of active futures to overwrite them on the next iteration.
            let active_futures = this.tasks.iter().filter(|t| t.is_some()).count();

            if active_futures > 0 {
                let _ = stdout.queue(cursor::MoveUp(active_futures as u16));
            }

            let _ = stdout.flush();
        }

        item
    }
}

fn remove_last_line(stdout: &mut std::io::Stdout) -> std::io::Result<()> {
    stdout
        .queue(terminal::Clear(terminal::ClearType::CurrentLine))?
        .queue(cursor::MoveUp(1))?
        .queue(terminal::Clear(terminal::ClearType::CurrentLine))?;

    Ok(())
}

fn reset(stdout: &mut std::io::Stdout) -> std::io::Result<()> {
    stdout
        .queue(cursor::Show)?
        .queue(terminal::Clear(terminal::ClearType::CurrentLine))?
        .queue(cursor::MoveToColumn(0))?;

    Ok(())
}
