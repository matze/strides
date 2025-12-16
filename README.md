# strides

[![Cargo](https://img.shields.io/crates/v/strides.svg)](https://crates.io/crates/strides)
[![Documentation](https://docs.rs/strides/badge.svg)](https://docs.rs/strides)

A command-line UI library to enhance async programs with progress bars and
spinners. It is async-first, opionated, far from feature complete and absolutely
not API stable. Use at your own risk.


## Examples

This is a simple example that reports the status of three concurrently running
futures with a customized spinner and elapsed time:

```rust
let mut group = Monitored::new(spinner::styles::DOTS_3.ticks())
    .with_spinner_style(owo_colors::Style::new().bright_purple().bold())
    .with_elapsed_time(true);

// Add three futures completing after 1, 2 and 3 seconds.
group.push(Timer::after(Duration::from_secs(1)), "one second".into());
group.push(Timer::after(Duration::from_secs(2)), "two seconds".into());
group.push(Timer::after(Duration::from_secs(3)), "three seconds".into());

future::block_on(async {
    group.for_each(|_| {}).await;
});
```

Go into the [examples](./examples/) directory for more elaborate examples.


## License

[MIT](./LICENSE)
