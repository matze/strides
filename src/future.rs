//! Spinner integration for futures.

pub mod monitored;

pub use monitored::Monitored;

use std::future::Future;
use std::io::Write;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_lite::{FutureExt as _, Stream};

use crate::spinner::Spinner;
use crate::term::clear_line;

/// Future for the [`progress`](FutureExt::progress) method.
pub struct Progress<F, T> {
    /// Wrapped future.
    inner: F,
    /// Spinner tick stream.
    ticks: T,
    /// Annotation for the future.
    message: String,
    /// Current spinner character.
    spinner: Option<char>,
}

impl<F, T> Future for Progress<F, T>
where
    F: Future + Unpin,
    T: Stream<Item = char> + Unpin,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let ticks = Pin::new(&mut this.ticks);

        if let Poll::Ready(spinner) = ticks.poll_next(cx) {
            this.spinner = spinner;
        }

        let _ = clear_line(&mut std::io::stdout());

        let item = this.inner.poll(cx);

        if matches!(item, Poll::Pending) {
            if let Some(spinner) = &this.spinner {
                print!("{spinner}");
            }

            print!(" {}", this.message);
        }

        std::io::stdout().flush().expect("flushing");
        item
    }
}

pub trait FutureExt: Future {
    fn progress(
        self,
        spinner: Spinner,
        message: impl std::fmt::Display,
    ) -> Progress<Self, impl Stream<Item = char>>
    where
        Self: Sized,
    {
        Progress {
            inner: self,
            ticks: spinner.ticks(),
            message: message.to_string(),
            spinner: None,
        }
    }
}

impl<F> FutureExt for F where F: Future {}
