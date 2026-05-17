# strides

[![Cargo](https://img.shields.io/crates/v/strides.svg)](https://crates.io/crates/strides)
[![Documentation](https://docs.rs/strides/badge.svg)](https://docs.rs/strides)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

A command-line UI library to enhance async programs with progress bars and
spinners.

The crate is built around two extension traits and two containers:

- [`FutureExt`] adds `.progress(theme)` (sugar for
  `.progressive().with_theme(theme)`), `.progressive()` (unconfigured;
  inherits the parent Group's theme or renders with `Theme::default()` if
  awaited directly), and the `.with_label(...)` / `.with_messages(...)` /
  `.with_progress(...)` / `.with_elapsed_time()` setters that implicitly lift a
  bare [`Future`] into a `ProgressFuture`.
- [`StreamExt`] adds `.progress(theme, ...)` / `.progress_bytes(theme, ...)`
  (sugar) and `.progressive()` / `.progressive_bytes()` (unconfigured) to
  any [`Stream`].
- [`future::Group`] runs many [`Future`]s concurrently, rendering one line
  per task.
- [`stream::Group`] runs many [`Stream`]s concurrently, rendering one line
  per stream.
- For byte-counted file copies and downloads, convert an `AsyncRead` to a
  byte stream (e.g. via [`tokio_util::io::ReaderStream`]) and use
  `.progress_bytes(...)`.

Each `.progress(...)` call animates automatically and returns a builder for
further customization. A [`Theme`] bundles a [`Spinner`], a [`Bar`] and a
[`Layout`], and is accepted everywhere a theme is expected. A [`Layout`] is an
ordered list of [`Segment`]s controlling the order, spacing and formatting of
each progress line. See [`spinner::styles`] and [`bar::styles`] for predefined
variants.

[`Future`]: https://doc.rust-lang.org/std/future/trait.Future.html
[`Stream`]: https://docs.rs/futures-lite/latest/futures_lite/stream/trait.Stream.html
[`FutureExt`]: https://docs.rs/strides/latest/strides/future/trait.FutureExt.html
[`StreamExt`]: https://docs.rs/strides/latest/strides/stream/trait.StreamExt.html
[`future::Group`]: https://docs.rs/strides/latest/strides/future/struct.Group.html
[`stream::Group`]: https://docs.rs/strides/latest/strides/stream/struct.Group.html
[`tokio_util::io::ReaderStream`]: https://docs.rs/tokio-util/latest/tokio_util/io/struct.ReaderStream.html
[`Theme`]: https://docs.rs/strides/latest/strides/struct.Theme.html
[`Spinner`]: https://docs.rs/strides/latest/strides/spinner/struct.Spinner.html
[`Bar`]: https://docs.rs/strides/latest/strides/bar/struct.Bar.html
[`Layout`]: https://docs.rs/strides/latest/strides/layout/struct.Layout.html
[`Segment`]: https://docs.rs/strides/latest/strides/layout/enum.Segment.html
[`spinner::styles`]: https://docs.rs/strides/latest/strides/spinner/styles/index.html
[`bar::styles`]: https://docs.rs/strides/latest/strides/bar/styles/index.html


## Example

This example demonstrates how to animate single futures, a group of futures and
a stream. Run it with `cargo run --example readme`.

```rust
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

    let theme = strides::Theme::default()
        .with_spinner(DOTS_3)
        .with_bar(strides::bar::styles::THIN_LINE);

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
```

See the [examples](./examples/) directory for more elaborate uses including
downloads, dynamic messages, per-task progress bars, and custom layouts.


## Minimum supported Rust version

strides requires Rust 1.85 or newer.


## License

[MIT](./LICENSE)
