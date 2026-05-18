//! Drive N concurrent futures and render them as a single progress line.
//!
//! [`Join`] owns its inner futures and polls them concurrently. Its [`Progressive::progress`] is
//! the completion fraction (`completed / total`), so the bar fills from 0/N to N/N as each future
//! resolves. Push one into a [`Group`](super::Group) to render many futures as one line alongside
//! other independent rows, or call [`with_theme`](Join::with_theme) for a self-contained row.

use std::fmt::Display;
use std::future::Future;
use std::io::IsTerminal;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_lite::stream::Pending;
use futures_lite::{stream, Stream};
use owo_colors::Style;
use pin_project_lite::pin_project;

use super::{Rendering, RenderingState};
use crate::line::{FrameContext, Line};
use crate::progressive::Progressive;
use crate::state::State;
use crate::term::CursorGuard;
use crate::Theme;

pin_project! {
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
        #[pin]
        messages: M,
        state: State,
        theme_override: Option<Theme<'a>>,
        spinner_style_override: Option<Style>,
        annotation_style_override: Option<Style>,
        rendering: RenderingState<'a>,
    }
}

/// Construct a [`Join`] from an iterable of futures sharing an `Output` type.
///
/// Accepts any [`IntoIterator`] of futures — `Vec<F>`, arrays, or any adapter chain. Without
/// [`with_theme`](Join::with_theme) the result inherits the parent `Group`'s theme when pushed,
/// with `with_theme` it renders standalone or overrides the Group's theme per-row.
pub fn join<I>(futs: I) -> Join<'static, I::Item>
where
    I: IntoIterator,
    I::Item: Future,
{
    Join::new(futs)
}

impl<F: Future> Join<'_, F> {
    /// Construct a `Join` with no theme set. Awaited directly it renders with [`Theme::default()`];
    /// chain [`with_theme`](Self::with_theme) for a custom theme, or push it into a
    /// [`Group`](super::Group) to inherit the Group's theme.
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
            theme_override: None,
            spinner_style_override: None,
            annotation_style_override: None,
            rendering: RenderingState::Pending,
        }
    }
}

impl<'a, F: Future, M> Join<'a, F, M> {
    /// Render this row with `theme`. Used for both the standalone path (drives the spinner /
    /// bar / cursor on its own line when awaited) and the per-row override path inside a
    /// [`Group`](super::Group).
    pub fn with_theme(mut self, theme: impl Into<Theme<'a>>) -> Self {
        self.theme_override = Some(theme.into());
        self
    }

    /// Apply `style` to the spinner character on this row, overriding the parent Group's default.
    pub fn with_spinner_style(mut self, style: Style) -> Self {
        self.spinner_style_override = Some(style);
        self
    }

    /// Apply `style` to the annotation (label) text on this row, overriding the parent Group's
    /// default.
    pub fn with_annotation_style(mut self, style: Style) -> Self {
        self.annotation_style_override = Some(style);
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
        S: Stream,
        S::Item: Display,
    {
        Join {
            futs: self.futs,
            results: self.results,
            completed: self.completed,
            total: self.total,
            messages,
            state: self.state,
            theme_override: self.theme_override,
            spinner_style_override: self.spinner_style_override,
            annotation_style_override: self.annotation_style_override,
            rendering: self.rendering,
        }
    }
}

impl<'a, F: Future, M> Progressive<'a> for Join<'a, F, M> {
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

    fn detach_rendering(&mut self) {
        self.rendering = RenderingState::Detached;
    }

    fn theme(&self) -> Option<&Theme<'a>> {
        self.theme_override.as_ref()
    }

    fn spinner_style(&self) -> Option<Style> {
        self.spinner_style_override
    }

    fn annotation_style(&self) -> Option<Style> {
        self.annotation_style_override
    }

    fn show_elapsed_time(&self) -> Option<bool> {
        if self.state.with_elapsed_time {
            Some(true)
        } else {
            None
        }
    }
}

impl<F, M> Future for Join<'_, F, M>
where
    F: Future,
    M: Stream,
    M::Item: Display,
{
    type Output = Vec<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        if matches!(this.rendering, RenderingState::Pending) {
            let theme = this.theme_override.clone().unwrap_or_default();
            let is_tty = std::io::stdout().is_terminal();
            let ticks = theme.spinner.ticks();
            let line = Line::new(&theme);
            // `standalone_render` reads progress from `State`, but `Join`'s `progress()` is
            // derived from `completed / total`. Mirror it into state so the bar renders.
            if *this.total > 0 {
                this.state
                    .set_progress(*this.completed as f64 / *this.total as f64);
            }
            *this.rendering = RenderingState::Active(Rendering {
                line,
                ticks,
                spinner_char: None,
                spinner_style: this.spinner_style_override.unwrap_or_default(),
                annotation_style: this.annotation_style_override.unwrap_or_default(),
                is_tty,
                _guard: CursorGuard { is_tty },
            });
        }

        let mut dirty = false;

        if let RenderingState::Active(r) = &mut *this.rendering {
            if let Poll::Ready(ch) = Pin::new(&mut r.ticks).poll_next(cx) {
                r.spinner_char = ch;
                dirty = true;
            }
        }

        while let Poll::Ready(Some(msg)) = this.messages.as_mut().poll_next(cx) {
            this.state.set_message(msg.to_string());
            dirty = true;
        }

        let mut i = 0;
        while i < this.futs.len() {
            match this.futs[i].as_mut().poll(cx) {
                Poll::Ready(out) => {
                    this.results.push(out);
                    drop(this.futs.swap_remove(i));
                    *this.completed += 1;
                    if *this.total > 0 {
                        this.state
                            .set_progress(*this.completed as f64 / *this.total as f64);
                    }
                    dirty = true;
                }
                Poll::Pending => i += 1,
            }
        }

        if let RenderingState::Active(r) = &mut *this.rendering {
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
                r.line.standalone_render(this.state, &frame, r.is_tty);
            }
        }

        if this.futs.is_empty() {
            if let RenderingState::Active(r) = &*this.rendering {
                Line::standalone_clear(r.is_tty);
            }
            Poll::Ready(std::mem::take(this.results))
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
