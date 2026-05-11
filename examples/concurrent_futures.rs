use std::time::Duration;

use async_io::Timer;
use async_signal::{Signal, Signals};
use futures_concurrency::future::Race as _;
use futures_lite::{StreamExt, future};
use strides::future::Group;
use strides::{spinner, term};

fn main() {
    // Create a group of futures that is tracked for completion and that uses the bright purple
    // dots to represent progress.
    let mut group = Group::new(spinner::styles::DOTS_3)
        .with_spinner_style(owo_colors::Style::new().bright_purple().bold())
        .with_elapsed_time(true);

    // Add three futures with varying completion durations.
    group.push(Timer::after(Duration::from_secs(1)), "one second".into());
    group.push(Timer::after(Duration::from_secs(2)), "two seconds".into());
    group.push(Timer::after(Duration::from_secs(3)), "three seconds".into());

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
