use std::time::Duration;

use async_io::Timer;
use futures_lite::StreamExt;
use strides::spinner::styles::{DOTS_3, SAND};

async fn animate_simple_future() {
    use strides::future::FutureExt as _;

    Timer::after(Duration::from_secs(2))
        .progress(DOTS_3)
        .with_label("some work going on for two seconds")
        .await;
}

async fn animate_two_futures_concurrently() {
    use strides::future::{FutureExt as _, Group};

    let mut group = Group::new(SAND).with_elapsed_time();
    group.push(Timer::after(Duration::from_secs(2)).with_label("two seconds"));
    group.push(Timer::after(Duration::from_secs(3)).with_label("three seconds"));

    group.for_each(|_| {}).await;
}

async fn animate_stream() {
    use strides::stream::StreamExt as _;

    let theme = strides::Theme::default().with_bar(strides::bar::styles::THIN_LINE);

    futures_lite::stream::iter(0..100)
        .progress(theme, |_, item| *item as f64 / 100.0)
        .with_label("streaming 100 items")
        .then(|item| async move {
            Timer::after(Duration::from_millis(20)).await;
            item
        })
        .for_each(|_| {})
        .await;
}

fn main() {
    futures_lite::future::block_on(async {
        animate_simple_future().await;
        animate_two_futures_concurrently().await;
        animate_stream().await;
    })
}
