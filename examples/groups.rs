//! Three ways to use [`future::Group`]:
//!
//! 1. One line per future: push each future as its own row.
//! 2. Per-line progress bars: each row drives its own bar via [`with_progress`].
//! 3. Many futures, one line: wrap a batch with [`future::join`] so the row's bar fills as each
//!    inner future completes.
//!
//! Run with `cargo run --example groups`.

use std::time::{Duration, Instant};

use async_io::Timer;
use async_signal::{Signal, Signals};
use futures::channel::mpsc;
use futures::SinkExt;
use futures_concurrency::future::Race as _;
use futures_lite::{future, StreamExt};
use strides::future::{join, FutureExt, Group};
use strides::{bar, term, Theme};

fn theme() -> Theme {
    let bar = bar::styles::THIN_LINE
        .with_filled_style(owo_colors::Style::new().bright_purple())
        .with_empty_style(owo_colors::Style::new().bright_black());

    Theme::default().with_bar(bar).with_bar_width(20)
}

async fn one_line_per_future() {
    let mut group = Group::new(theme()).with_elapsed_time();
    group.push(Timer::after(Duration::from_secs(1)).with_label("one second"));
    group.push(Timer::after(Duration::from_secs(2)).with_label("two seconds"));
    group.push(Timer::after(Duration::from_secs(3)).with_label("three seconds"));
    group.for_each(|_| {}).await;
}

async fn per_line_progress() {
    let mut group = Group::new(theme());

    for (label, secs) in [("qux", 2u64), ("foo", 3), ("bar", 4)] {
        let (mut tx, rx) = mpsc::unbounded::<f64>();
        let secs = Duration::from_secs(secs);
        let work = async move {
            let start = Instant::now();
            while start.elapsed() < secs {
                let _ = tx
                    .send(start.elapsed().as_secs_f64() / secs.as_secs_f64())
                    .await;
                Timer::after(Duration::from_millis(50)).await;
            }
        };
        group.push(work.with_label(label).with_progress(rx));
    }

    group.for_each(|_| {}).await;
}

async fn many_futures_one_line() {
    let checks = (1..=6).map(|i| async move {
        Timer::after(Duration::from_millis(300 + i * 100)).await;
        i
    });

    let mut group = Group::new(theme()).with_elapsed_time();
    group.push(join(checks).with_label("checking"));
    group.for_each(|_| {}).await;
}

fn main() {
    let mut signals = Signals::new([Signal::Int]).expect("signal handler");

    future::block_on(async {
        let work = async {
            one_line_per_future().await;
            per_line_progress().await;
            many_futures_one_line().await;
        };

        let on_interrupt = async {
            let _ = signals.next().await;
            let _ = term::reset();
        };

        (work, on_interrupt).race().await;
    });
}
