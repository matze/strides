//! Copy bytes from `/dev/urandom` into `/dev/null` for five seconds, showing live throughput.
//!
//! Demonstrates how to layer progress on an `AsyncRead` without strides shipping its own I/O
//! wrappers: convert the reader to a byte stream with `tokio_util::io::ReaderStream` and feed it
//! through [`StreamExt::progress_bytes`](strides::stream::StreamExt::progress_bytes). There is no
//! known total length, so the layout omits the bar and renders only spinner, elapsed time, bytes
//! transferred and rate.
//!
//! Run with: `cargo run --example iocopy`.

use std::time::Duration;

use strides::layout::{Layout, Segment};
use strides::stream::StreamExt as _;
use strides::{spinner, Theme};
use tokio::io;
use tokio_util::io::{ReaderStream, StreamReader};

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

    let source = tokio::fs::File::open("/dev/urandom").await?;
    let mut sink = tokio::fs::File::create("/dev/null").await?;

    let tracked = ReaderStream::new(source)
        .progress_bytes(theme, |chunk| {
            chunk.as_ref().map(|c| c.len() as u64).unwrap_or(0)
        })
        .with_elapsed_time();

    let mut source = StreamReader::new(tracked);

    let _ = tokio::time::timeout(Duration::from_secs(5), io::copy(&mut source, &mut sink)).await;

    Ok(())
}
