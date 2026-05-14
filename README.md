# strides

[![Cargo](https://img.shields.io/crates/v/strides.svg)](https://crates.io/crates/strides)
[![Documentation](https://docs.rs/strides/badge.svg)](https://docs.rs/strides)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

A command-line UI library to enhance async programs with progress bars and
spinners. It is async-first, opinionated, far from feature complete and
absolutely not API stable. Use at your own risk.

The crate is built around two extension traits and one container:

[`FutureExt`] adds `.progress(theme)` to any [`Future`], animating it
automatically and returning a builder for further customization. [`StreamExt`]
adds `.progress(theme, fraction_fn)` to any [`Stream`], animating it and
returning a builder for further customization. [`Group`] runs many futures
concurrently and renders one line per task.

A [`Theme`] bundles a [`Spinner`] and a [`Bar`] and is accepted everywhere a
theme is expected. See [`spinner::styles`] and [`bar::styles`] for predefined
variants.

[`Future`]: https://doc.rust-lang.org/std/future/trait.Future.html
[`Stream`]: https://docs.rs/futures-lite/latest/futures_lite/stream/trait.Stream.html
[`FutureExt`]: https://docs.rs/strides/latest/strides/future/trait.FutureExt.html
[`StreamExt`]: https://docs.rs/strides/latest/strides/stream/trait.StreamExt.html
[`Group`]: https://docs.rs/strides/latest/strides/future/struct.Group.html
[`Theme`]: https://docs.rs/strides/latest/strides/struct.Theme.html
[`Spinner`]: https://docs.rs/strides/latest/strides/spinner/struct.Spinner.html
[`Bar`]: https://docs.rs/strides/latest/strides/bar/struct.Bar.html
[`spinner::styles`]: https://docs.rs/strides/latest/strides/spinner/styles/index.html
[`bar::styles`]: https://docs.rs/strides/latest/strides/bar/styles/index.html


## Example

Three concurrently running futures with a customized spinner and elapsed time:

```rust
use std::time::Duration;
use async_io::Timer;
use futures_lite::{StreamExt, future};
use strides::future::{FutureExt, Group};
use strides::spinner;

let mut group = Group::new(spinner::styles::DOTS_3)
    .with_spinner_style(owo_colors::Style::new().bright_purple().bold())
    .with_elapsed_time(true);

group.push(Timer::after(Duration::from_secs(1)).with_label("one second"));
group.push(Timer::after(Duration::from_secs(2)).with_label("two seconds"));
group.push(Timer::after(Duration::from_secs(3)).with_label("three seconds"));

future::block_on(async {
    group.for_each(|_| {}).await;
});
```

See the [examples](./examples/) directory for more elaborate uses including
downloads, dynamic messages, and per-task progress bars.


## Minimum supported Rust version

strides requires Rust 1.85 or newer.


## License

[MIT](./LICENSE)
