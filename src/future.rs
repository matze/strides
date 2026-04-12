//! Spinner integration for futures.

pub mod monitored;

pub use monitored::Monitored;

use std::future::Future;
use std::io::Write;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_lite::{FutureExt as _, Stream, stream};

use crate::bar::Bar;
use crate::style::ProgressStyle;
use crate::term::clear_line;

/// Future for the [`progress`](FutureExt::progress) and
/// [`progress_with_messages`](FutureExt::progress_with_messages) methods.
pub struct Progress<'a, F, T, M> {
    /// Wrapped future.
    inner: F,
    /// Spinner tick stream.
    ticks: T,
    /// Messages stream.
    messages: M,
    /// Progress bar style.
    bar: Bar<'a>,
    /// Current annotation for the future.
    message: Option<String>,
    /// Current spinner character.
    spinner: Option<char>,
}

impl<F, T, M, D> Future for Progress<'_, F, T, M>
where
    F: Future + Unpin,
    T: Stream<Item = char> + Unpin,
    M: Stream<Item = D> + Unpin,
    D: std::fmt::Display,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let ticks = Pin::new(&mut this.ticks);

        if let Poll::Ready(spinner) = ticks.poll_next(cx) {
            this.spinner = spinner;
        }

        let messages = Pin::new(&mut this.messages);

        if let Poll::Ready(message) = messages.poll_next(cx) {
            this.message = message.map(|m| m.to_string());
        }

        let _ = clear_line(&mut std::io::stdout());

        let item = this.inner.poll(cx);

        if matches!(item, Poll::Pending) {
            if let Some(spinner) = &this.spinner {
                print!("{spinner} ");
            }

            if let Some(message) = &this.message {
                print!("{message}");
            }
        }

        std::io::stdout().flush().expect("flushing");
        item
    }
}

pub trait FutureExt: Future {
    fn progress<'a>(
        self,
        style: impl Into<ProgressStyle<'a>>,
        message: impl std::fmt::Display,
    ) -> Progress<'a, Self, impl Stream<Item = char>, impl Stream<Item = impl std::fmt::Display>>
    where
        Self: Sized,
    {
        let style = style.into();

        Progress {
            inner: self,
            ticks: style.spinner.ticks(),
            messages: stream::pending::<&'static str>(),
            bar: style.bar,
            message: Some(message.to_string()),
            spinner: None,
        }
    }

    fn progress_with_messages<'a>(
        self,
        style: impl Into<ProgressStyle<'a>>,
        messages: impl Stream<Item = impl std::fmt::Display>,
    ) -> Progress<'a, Self, impl Stream<Item = char>, impl Stream<Item = impl std::fmt::Display>>
    where
        Self: Sized,
    {
        let style = style.into();

        Progress {
            inner: self,
            ticks: style.spinner.ticks(),
            messages,
            bar: style.bar,
            message: None,
            spinner: None,
        }
    }
}

impl<F> FutureExt for F where F: Future {}
