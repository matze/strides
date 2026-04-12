use std::time::{Duration, Instant};

use futures_lite::{Stream, StreamExt as _, future, stream};
use strides::future::FutureExt;
use strides::spinner;

fn throttle<I>(s: impl Stream<Item = I>, interval: Duration) -> impl Stream<Item = I> {
    s.zip(async_io::Timer::interval_at(Instant::now(), interval))
        .map(|(item, _)| item)
}

fn main() {
    // Define a stream of messages that are emitted every second.
    let messages = throttle(
        stream::iter(["connecting ...", "fetching data ...", "processing ...", "wrapping up ..."]),
        Duration::from_secs(1),
    );

    future::block_on(async {
        // Await a long-running future with dynamic status messages.
        std::pin::pin!(async {
            async_io::Timer::after(Duration::from_secs(5)).await;
            42
        })
        .progress_with_messages(spinner::styles::SAND, messages)
        .await;

        println!("Done!");
    });
}
