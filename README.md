# strides

[![Cargo](https://img.shields.io/crates/v/strides.svg)](https://crates.io/crates/strides)
[![Documentation](https://docs.rs/strides/badge.svg)](https://docs.rs/strides)

A command-line UI library to enhance async programs with progress bars and
spinners. It is async-first, opinionated, far from feature complete and
absolutely not API stable. Use at your own risk.

The crate is built around two extension traits and one container:

[`future::FutureExt`] adds `.progress(theme)` to any [`Future`], returning a
builder that composes optional capabilities `with_message`, `with_messages` and
`with_fraction`. [`stream::StreamExt`] adds `.progress(theme, fraction_fn)` to
any [`Stream`], returning a builder with `with_messages`. [`future::Group`] runs
many futures concurrently and renders one line per task. Bare futures convert
into [`future::Task`] implicitly; configure with `with_label`, `with_messages`
and `with_progress` (mirrored on `FutureExt`).

A [`Theme`] bundles a [`spinner::Spinner`] and a [`bar::Bar`] and is
accepted everywhere a theme is expected; bare spinners convert
implicitly. See [`spinner::styles`] and [`bar::styles`] for predefined
variants.

```rust,ignore
use futures_lite::StreamExt as _;
use strides::stream::StreamExt;
```

[`Future`]: https://doc.rust-lang.org/std/future/trait.Future.html
[`Stream`]: https://docs.rs/futures-lite/latest/futures_lite/stream/trait.Stream.html
[`future::FutureExt`]: https://docs.rs/strides/latest/strides/future/trait.FutureExt.html
[`stream::StreamExt`]: https://docs.rs/strides/latest/strides/stream/trait.StreamExt.html
[`future::Group`]: https://docs.rs/strides/latest/strides/future/struct.Group.html
[`future::Task`]: https://docs.rs/strides/latest/strides/future/struct.Task.html
[`Theme`]: https://docs.rs/strides/latest/strides/struct.Theme.html
[`spinner::Spinner`]: https://docs.rs/strides/latest/strides/spinner/struct.Spinner.html
[`bar::Bar`]: https://docs.rs/strides/latest/strides/bar/struct.Bar.html
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


## License

[MIT](./LICENSE)
