//! Drive N concurrent futures and render them as a single progress line.
//!
//! [`Join`] owns its inner futures and polls them concurrently. Its
//! [`Progressive::progress`](crate::progressive::Progressive::progress) is the completion
//! fraction (`completed / total`), so the bar fills from 0/N to N/N as each future resolves.
//! Push one into a [`Group`](super::Group) to render many futures as one line alongside other
//! independent rows, or call [`standalone`](Join::standalone) for a self-contained row.

use std::fmt::Display;
use std::future::Future;
use std::io::IsTerminal;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_lite::stream::Pending;
use futures_lite::{stream, Stream};
use owo_colors::Style;

use super::Rendering;
use crate::line::{FrameContext, Line};
use crate::progressive::Progressive;
use crate::state::State;
use crate::term::CursorGuard;
use crate::Theme;

/// N concurrent futures collapsed into a single [`Progressive`] row.
///
/// The bar fills from 0/N to N/N as each inner future completes. Results are collected in
/// completion order. With zero inputs, awaiting resolves immediately with an empty `Vec` and no
/// progress is reported.
pub struct Join<'a, F: Future, M = Pending<&'static str>> {
    futs: Vec<Pin<Box<F>>>,
    results: Vec<F::Output>,
    completed: usize,
    total: usize,
    messages: M,
    state: State,
    rendering: Option<Rendering<'a>>,
}

/// Construct a tracked-only [`Join`] for use with [`Group::push`](super::Group::push).
///
/// Accepts any [`IntoIterator`] of futures sharing an `Output` type — `Vec<F>`, arrays, or any
/// adapter chain. For a standalone row, chain [`.standalone(theme)`](Join::standalone) onto the
/// result.
pub fn join<I>(futs: I) -> Join<'static, I::Item>
where
    I: IntoIterator,
    I::Item: Future,
{
    Join::new(futs)
}

impl<'a, F: Future> Join<'a, F> {
    /// Tracked-only constructor. The returned `Join` does not render on its own — push it into a
    /// [`Group`](super::Group).
    pub fn new<I>(futs: I) -> Self
    where
        I: IntoIterator<Item = F>,
    {
        let futs: Vec<Pin<Box<F>>> = futs.into_iter().map(Box::pin).collect();
        let total = futs.len();
        Self {
            results: Vec::with_capacity(total),
            futs,
            completed: 0,
            total,
            messages: stream::pending(),
            state: State::new(),
            rendering: None,
        }
    }
}

impl<'a, F: Future, M> Join<'a, F, M> {
    /// Upgrade this `Join` into a standalone row rendered with `theme`. The returned value drives
    /// its own terminal line when awaited.
    pub fn standalone(mut self, theme: impl Into<Theme<'a>>) -> Self {
        let theme = theme.into();
        let is_tty = std::io::stdout().is_terminal();
        let ticks = theme.spinner.ticks();
        let line = Line::new(&theme);
        self.rendering = Some(Rendering {
            line,
            ticks,
            spinner_char: None,
            spinner_style: Style::new(),
            annotation_style: Style::new(),
            is_tty,
            _guard: CursorGuard { is_tty },
        });
        self
    }

    /// Set the static label shown in the [`Label`](crate::layout::Segment::Label) segment.
    pub fn with_label(mut self, label: impl Display) -> Self {
        self.state.set_label(label.to_string());
        self
    }

    /// Prepend the elapsed time to the line.
    pub fn with_elapsed_time(mut self) -> Self {
        self.state.enable_elapsed_time();
        self
    }

    /// Replace the displayed message each time `messages` yields a value.
    pub fn with_messages<S>(self, messages: S) -> Join<'a, F, S>
    where
        S: Stream + Unpin,
        S::Item: Display,
    {
        Join {
            futs: self.futs,
            results: self.results,
            completed: self.completed,
            total: self.total,
            messages,
            state: self.state,
            rendering: self.rendering,
        }
    }
}

impl<F: Future, M> Progressive for Join<'_, F, M> {
    fn label(&self) -> Option<&str> {
        self.state.label()
    }

    fn message(&self) -> Option<&str> {
        self.state.message()
    }

    fn progress(&self) -> Option<f64> {
        if self.total == 0 {
            None
        } else {
            Some(self.completed as f64 / self.total as f64)
        }
    }
}

impl<F, M> Future for Join<'_, F, M>
where
    F: Future,
    F::Output: Unpin,
    M: Stream + Unpin,
    M::Item: Display,
{
    type Output = Vec<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut dirty = false;

        if let Some(r) = this.rendering.as_mut() {
            if let Poll::Ready(ch) = Pin::new(&mut r.ticks).poll_next(cx) {
                r.spinner_char = ch;
                dirty = true;
            }
        }

        while let Poll::Ready(Some(msg)) = Pin::new(&mut this.messages).poll_next(cx) {
            this.state.set_message(msg.to_string());
            dirty = true;
        }

        let mut i = 0;
        while i < this.futs.len() {
            match this.futs[i].as_mut().poll(cx) {
                Poll::Ready(out) => {
                    this.results.push(out);
                    let _ = this.futs.swap_remove(i);
                    this.completed += 1;
                    if this.total > 0 {
                        this.state
                            .set_progress(this.completed as f64 / this.total as f64);
                    }
                    dirty = true;
                }
                Poll::Pending => i += 1,
            }
        }

        if let Some(r) = this.rendering.as_mut() {
            if !this.futs.is_empty() && dirty {
                let elapsed = if this.state.with_elapsed_time {
                    this.state.elapsed()
                } else {
                    Duration::ZERO
                };
                let frame = FrameContext {
                    spinner_char: r.spinner_char,
                    elapsed,
                    show_elapsed: this.state.with_elapsed_time,
                    spinner_style: r.spinner_style,
                    annotation_style: r.annotation_style,
                };
                r.line.standalone_render(&this.state, &frame, r.is_tty);
            }
        }

        if this.futs.is_empty() {
            if let Some(r) = this.rendering.as_ref() {
                Line::standalone_clear(r.is_tty);
            }
            Poll::Ready(std::mem::take(&mut this.results))
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_io::Timer;
    use futures_lite::future;

    use super::*;

    #[test]
    fn empty_input_resolves_immediately() {
        future::block_on(async {
            let results: Vec<()> = join(Vec::<futures_lite::future::Ready<()>>::new()).await;
            assert!(results.is_empty());
        });
    }

    #[test]
    fn returns_results_in_completion_order() {
        future::block_on(async {
            let futs = [(60, "slow"), (20, "fast"), (40, "medium")]
                .into_iter()
                .map(|(ms, name)| async move {
                    Timer::after(Duration::from_millis(ms)).await;
                    name
                });
            let results = join(futs).await;
            assert_eq!(results, vec!["fast", "medium", "slow"]);
        });
    }

    #[test]
    fn progress_reflects_completion_fraction() {
        let j: Join<'_, futures_lite::future::Ready<()>> = Join::new(Vec::new());
        assert!(j.progress().is_none());

        let j = join(vec![futures_lite::future::ready(()); 4]);
        assert_eq!(j.progress(), Some(0.0));
    }
}
