use std::time::Duration;
use std::time::Instant;

use async_signal::{Signal, Signals};
use futures_concurrency::future::Race as _;
use futures_lite::StreamExt as _;
use futures_lite::{Stream, future, stream};
use strides::stream::StreamExt;
use strides::{Theme, bar, spinner, term};

fn throttle<I>(s: impl Stream<Item = I>, interval: Duration) -> impl Stream<Item = I> {
    s.zip(async_io::Timer::interval_at(Instant::now(), interval))
        .map(|(item, _)| item)
}

fn main() {
    // Define a stream of numbers 0..100 that are emitted every 30ms.
    let ticks = throttle(stream::iter(0..100), Duration::from_millis(30));

    // Define a stream of messages that are emitted every second.
    let messages = throttle(
        stream::iter(["hello ...", "computing ...", "almost there ...", "done"]),
        Duration::from_secs(1),
    );

    // Set up our theme with parallelogram bar and sand spinner. The stream gives no size
    // hint, thus we need to set the number of expected items manually.
    let theme = Theme::default()
        .with_bar(bar::styles::PARALLELOGRAM)
        .with_spinner(spinner::styles::SAND);

    let mut signals = Signals::new([Signal::Int]).expect("signal handler");

    future::block_on(async {
        let work = async {
            let sum = ticks
                .progress_with_messages(theme, |index, _| index as f64 / 100.0, messages)
                .fold(0, |acc, x| acc + x)
                .await;

            println!("Sum is {sum}, Gauss was right");
        };

        let on_interrupt = async {
            let _ = signals.next().await;
            let _ = term::reset();
        };

        (work, on_interrupt).race().await;
    });
}
