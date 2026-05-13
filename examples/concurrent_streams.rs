use std::time::{Duration, Instant};

use async_io::Timer;
use async_signal::{Signal, Signals};
use futures::SinkExt;
use futures::channel::mpsc;
use futures_concurrency::future::Race as _;
use futures_lite::{StreamExt, future, stream};
use strides::future::{FutureExt, Group};
use strides::{Theme, bar, spinner, term};

fn main() {
    let bar = bar::styles::THIN_LINE
        .with_filled_style(owo_colors::Style::new().bright_purple())
        .with_empty_style(owo_colors::Style::new().bright_black());

    let theme = Theme::default()
        .with_spinner(spinner::styles::DOTS_3)
        .with_bar(bar)
        .with_bar_width(20);

    let mut group =
        Group::new(theme).with_spinner_style(owo_colors::Style::new().bright_green().bold());

    // Three "downloads" of different durations, each emitting a fake byte stream.
    for (label, message, secs) in [
        ("X", "alpha", Duration::from_secs(2)),
        ("Y", "beta", Duration::from_secs(3)),
        ("Z", "gamma", Duration::from_secs(4)),
    ] {
        let (mut tx, rx) = mpsc::unbounded::<f64>();

        let work = async move {
            let start = Instant::now();

            while start.elapsed() < secs {
                let p = start.elapsed().as_secs_f64() / secs.as_secs_f64();
                let _ = tx.send(p.min(1.0)).await;
                Timer::after(Duration::from_millis(50)).await;
            }
        };

        group.push(
            Box::pin(work)
                .with_label(label)
                .with_messages(stream::once(message))
                .with_progress(rx),
        );
    }

    let mut signals = Signals::new([Signal::Int]).expect("signal handler");

    future::block_on(async {
        let work = async {
            group.for_each(|_| {}).await;
        };

        let on_interrupt = async {
            let _ = signals.next().await;
            let _ = term::reset();
        };

        (work, on_interrupt).race().await;
    });
}
