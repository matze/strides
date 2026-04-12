use std::future::Future;
use std::task::{Context, Poll};
use std::time::Instant;
use std::{io::Write, pin::Pin};

use crossterm::{QueueableCommand, cursor, terminal};
use futures::stream::FuturesUnordered;
use futures_lite::{FutureExt as _, Stream};
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

/// Wrapper around a [`futures::stream::FuturesUnordered`] that monitors completion status of futures.
pub struct Monitored<'a, F> {
    /// Group of futures.
    inner: FuturesUnordered<Annotated<F>>,
    /// Spinner tick stream.
    ticks: Ticks<'a>,
    /// Annotation mapping from id (index) to string. It is reset when the corresponding future
    /// finished.
    annotations: Vec<Option<String>>,
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
}

impl<'a, F> Monitored<'a, F>
where
    F: Future,
{
    pub fn new(spinner: Spinner<'a>) -> Self {
        Self {
            inner: FuturesUnordered::new(),
            ticks: spinner.ticks(),
            annotations: Vec::new(),
            annotation_style: owo_colors::Style::new(),
            spinner: None,
            spinner_style: owo_colors::Style::new(),
            with_elapsed_time: false,
            start: None,
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

    /// Add `fut` to the monitored group and annotate it with `annotation`.
    pub fn push(&mut self, fut: F, annotation: String) {
        let id = self.annotations.len();
        self.annotations.push(Some(annotation));
        self.inner.push(Annotated::new(fut, id));
    }
}

impl<F> Stream for Monitored<'_, F>
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
        }

        let mut stdout = std::io::stdout();
        let _ = stdout.queue(cursor::Hide);

        for annotation in &this.annotations {
            let _ = clear_line(&mut stdout);

            if let Some(spinner) = &this.spinner {
                print!("{} ", spinner.style(this.spinner_style));
            }

            if this.with_elapsed_time {
                print!("[{:.2}s] ", elapsed.as_secs_f64());
            }

            if let Some(annotation) = annotation {
                println!("{}", annotation.style(this.annotation_style));
            }
        }

        let item = match inner.poll_next(cx) {
            Poll::Ready(Some((output, id))) => {
                this.annotations[id] = None;

                // Clear last and go up one line because we have one less future to track.
                let _ = remove_last_line(&mut stdout);
                Poll::Ready(Some(output))
            }
            Poll::Ready(None) => {
                let _ = reset(&mut stdout);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        };

        if !matches!(item, Poll::Ready(None)) {
            // Go up by number of active futures to overwrite them on the next iteration.
            let active_futures = this.annotations.iter().filter(|a| a.is_some()).count();

            if active_futures > 0 {
                let _ = stdout.queue(cursor::MoveUp(active_futures as u16));
            }
        }

        let _ = stdout.flush();

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
