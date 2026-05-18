use std::time::{Duration, Instant};

use async_signal::{Signal, Signals};
use futures_concurrency::future::Race as _;
use futures_lite::{future, stream, Stream, StreamExt as _};
use strides::future::FutureExt;
use strides::{spinner, term};

fn throttle<I>(s: impl Stream<Item = I>, interval: Duration) -> impl Stream<Item = I> {
    s.zip(async_io::Timer::interval_at(Instant::now(), interval))
        .map(|(item, _)| item)
}

fn main() {
    // Define a stream of messages that are emitted every second.
    let messages = throttle(
        stream::iter([
            "connecting ...",
            "fetching data ...",
            "processing ...",
            "wrapping up ...",
        ]),
        Duration::from_secs(1),
    );

    let mut signals = Signals::new([Signal::Int]).expect("signal handler");

    future::block_on(async {
        let work = async {
            // Await a long-running future with dynamic status messages.
            async {
                async_io::Timer::after(Duration::from_secs(5)).await;
                42
            }
            .progress(spinner::styles::SAND)
            .with_messages(messages)
            .await;

            println!("Done!");
        };

        let on_interrupt = async {
            let _ = signals.next().await;
            let _ = term::reset();
        };

        (work, on_interrupt).race().await;
    });
}
