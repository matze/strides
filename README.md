# strides

[![Cargo](https://img.shields.io/crates/v/strides.svg)](https://crates.io/crates/strides)
[![Documentation](https://docs.rs/strides/badge.svg)](https://docs.rs/strides)

A command-line UI library to enhance async programs with progress bars and
spinners. It is async-first, opinionated, far from feature complete and
absolutely not API stable. Use at your own risk.

The crate is built around two extension traits and one container:

- [`future::FutureExt`] adds `.progress()` and `.progress_with_messages()` to
  any [`Future`], rendering a spinner (and optional bar) while it is pending.
- [`stream::StreamExt`] adds the same methods to a [`Stream`], driving the bar
  from a closure called for every item.
- [`future::Group`] runs many futures concurrently and renders one line per
  task with optional per-task progress and dynamic messages.

A [`Theme`] bundles a [`spinner::Spinner`] and a [`bar::Bar`] and is
accepted everywhere a theme is expected; bare spinners convert
implicitly. See [`spinner::styles`] and [`bar::styles`] for predefined
variants.

[`Future`]: https://doc.rust-lang.org/std/future/trait.Future.html
[`Stream`]: https://docs.rs/futures-lite/latest/futures_lite/stream/trait.Stream.html
[`future::FutureExt`]: https://docs.rs/strides/latest/strides/future/trait.FutureExt.html
[`stream::StreamExt`]: https://docs.rs/strides/latest/strides/stream/trait.StreamExt.html
[`future::Group`]: https://docs.rs/strides/latest/strides/future/struct.Group.html
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
use strides::future::Group;
use strides::spinner;

let mut group = Group::new(spinner::styles::DOTS_3)
    .with_spinner_style(owo_colors::Style::new().bright_purple().bold())
    .with_elapsed_time(true);

group.push(Timer::after(Duration::from_secs(1)), "one second".into());
group.push(Timer::after(Duration::from_secs(2)), "two seconds".into());
group.push(Timer::after(Duration::from_secs(3)), "three seconds".into());

future::block_on(async {
    group.for_each(|_| {}).await;
});
```

See the [examples](./examples/) directory for more elaborate uses including
downloads, dynamic messages, and per-task progress bars.


## License

[MIT](./LICENSE)
