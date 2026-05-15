//! Copy bytes from `/dev/urandom` into `/dev/null` for five seconds, showing live throughput.
//!
//! Demonstrates the tokio `AsyncRead` progress wrapper. There is no known total length, so the
//! layout omits the bar and renders only spinner, elapsed time, bytes transferred and rate.
//!
//! Run with: `cargo run --example iocopy --features tokio`.

use std::time::Duration;

use strides::io::tokio::AsyncReadProgressExt as _;
use strides::layout::{Layout, Segment};
use strides::{spinner, Theme};
use tokio::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    let layout = Layout::new(&[])
        .with_segment(Segment::spinner())
        .with_segment(Segment::elapsed().with_border("[", "]"))
        .with_segment(Segment::bytes())
        .with_segment(Segment::literal("@"))
        .with_segment(Segment::rate());

    let theme = Theme::default()
        .with_spinner(spinner::styles::DOTS_3)
        .with_layout(layout);

    let mut source = tokio::fs::File::open("/dev/urandom")
        .await?
        .progress(theme)
        .with_elapsed_time();

    let mut sink = tokio::fs::File::create("/dev/null").await?;

    let _ = tokio::time::timeout(Duration::from_secs(5), io::copy(&mut source, &mut sink)).await;

    Ok(())
}
