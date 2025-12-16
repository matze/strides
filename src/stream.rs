//! Progress bar extension for streams.

use std::io::Write;
use std::pin::Pin;
use std::task::Poll;

use futures_lite::{Stream, stream};

use crate::bar::Bar;
use crate::spinner::Spinner;
use crate::term::clear_line;

#[derive(Clone)]
pub struct ProgressStyle<'a> {
    /// Spinner style to indicate activity.
    spinner: Spinner<'a>,
    /// Bar style to indicate progress.
    bar: Bar<'a>,
}

impl<'a> ProgressStyle<'a> {
    pub fn new() -> Self {
        Self {
            spinner: Spinner::inactive(),
            bar: Bar::default(),
        }
    }

    pub fn with_spinner(mut self, spinner: Spinner<'a>) -> Self {
        self.spinner = spinner;
        self
    }

    pub fn with_bar(mut self, bar: Bar<'a>) -> Self {
        self.bar = bar;
        self
    }
}

/// Stream for the [`progress`](StreamExt::progress) and
/// [`progress_with_messages`](StreamExt::progress_with_messages) methods.
pub struct Progress<'a, S, F, T, M> {
    /// Wrapped stream.
    inner: S,
    /// Progress bar style
    bar: Bar<'a>,
    /// Closure to compute the progress.
    progress_fn: F,
    /// Spinner tick stream.
    ticks: T,
    /// Messages stream.
    messages: M,
    /// Current index
    current: usize,
    /// Current spinner character.
    spinner: Option<char>,
    /// Current message.
    message: Option<String>,
}

impl<'a, S, F, T, M, D> Stream for Progress<'a, S, F, T, M>
where
    S: Stream + Unpin,
    F: FnMut(usize, &S::Item) -> f64 + Unpin,
    T: Stream<Item = char> + Unpin,
    M: Stream<Item = D> + Unpin,
    D: std::fmt::Display,
{
    type Item = S::Item;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let inner = Pin::new(&mut this.inner);
        let ticks = Pin::new(&mut this.ticks);
        let messages = Pin::new(&mut this.messages);

        // Poll the spinner stream.
        if let Poll::Ready(spinner) = ticks.poll_next(cx) {
            this.spinner = spinner;
        }

        // Poll the message stream.
        if let Poll::Ready(message) = messages.poll_next(cx) {
            this.message = message.map(|m| m.to_string());
        }

        // Poll the wrapped stream.
        match inner.poll_next(cx) {
            Poll::Ready(Some(item)) => {
                this.current += 1;

                let _ = clear_line(&mut std::io::stdout());

                if let Some(spinner) = &this.spinner {
                    print!("{spinner} ");
                }

                let completed = (this.progress_fn)(this.current, &item);

                print!("{}", this.bar.render(40, completed));

                if let Some(message) = &this.message {
                    print!(" {message}");
                }

                std::io::stdout().flush().expect("flushing");
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                // Stream ended, so clear output.
                let _ = clear_line(&mut std::io::stdout());
                std::io::stdout().flush().expect("flushing");
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

pub trait StreamExt<'a, F>: Stream {
    fn progress(
        self,
        progress: ProgressStyle<'a>,
        progress_fn: F,
    ) -> Progress<
        'a,
        Self,
        F,
        impl Stream<Item = char> + use<'a, F, Self>,
        impl Stream<Item = impl std::fmt::Display>,
    >
    where
        Self: Sized,
        F: FnMut(usize, &Self::Item) -> f64 + Unpin,
    {
        let ProgressStyle { bar, spinner } = progress;

        Progress {
            inner: self,
            progress_fn,
            bar,
            ticks: spinner.ticks(),
            messages: stream::pending::<&'static str>(),
            current: 0,
            spinner: None,
            message: None,
        }
    }

    fn progress_with_messages(
        self,
        progress: ProgressStyle<'a>,
        progress_fn: F,
        messages: impl Stream<Item = impl std::fmt::Display>,
    ) -> Progress<'a, Self, F, impl Stream<Item = char>, impl Stream<Item = impl std::fmt::Display>>
    where
        Self: Sized,
        F: FnMut(usize, &Self::Item) -> f64 + Unpin,
    {
        let ProgressStyle { bar, spinner } = progress;

        Progress {
            inner: self,
            progress_fn,
            bar,
            ticks: spinner.ticks(),
            messages,
            current: 0,
            spinner: None,
            message: None,
        }
    }
}

impl<'a, S, F> StreamExt<'a, F> for S where S: Stream {}
