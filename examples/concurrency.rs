use std::time::Duration;

use async_io::Timer;
use futures_lite::{StreamExt, future};
use strides::future::Group;
use strides::spinner;

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

    future::block_on(async {
        group.for_each(|_| {}).await;
    });
}
