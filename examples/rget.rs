use anyhow::anyhow;
use async_signal::{Signal, Signals};
use clap::Parser;
use futures::{StreamExt as _, TryStreamExt};
use futures_concurrency::future::Race as _;
use strides::layout::{Layout, Segment};
use strides::stream::StreamExt as _;
use strides::{bar, term, Theme};
use tokio_util::codec::{BytesCodec, FramedWrite};

#[derive(Parser, Debug)]
struct Args {
    /// URL to fetch content from.
    url: reqwest::Url,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let name = args
        .url
        .path_segments()
        .ok_or_else(|| anyhow!("{} cannot be a base", args.url))?
        .last()
        .map(String::from)
        .ok_or_else(|| anyhow!("failed to convert segment to string"))?;

    let response = reqwest::get(args.url).await?;
    let length = response.content_length().unwrap_or_default();

    let layout = Layout::new(&[])
        .with_segment(Segment::spinner())
        .with_segment(Segment::bar())
        .with_segment(Segment::bytes())
        .with_segment(Segment::literal("@"))
        .with_segment(Segment::rate())
        .with_segment(Segment::literal("·"))
        .with_segment(Segment::eta());

    let theme = Theme::default()
        .with_bar(bar::styles::SHADED)
        .with_layout(layout);

    let stream = response
        .bytes_stream()
        .progress_bytes(theme, |item| {
            item.as_ref().map(|c| c.len() as u64).unwrap_or(0)
        })
        .with_len(length)
        .map_err(std::io::Error::other);

    let file = tokio::fs::File::create_new(name).await?;
    let writer = FramedWrite::new(file, BytesCodec::new());

    let mut signals = Signals::new([Signal::Int])?;

    let work = async { stream.forward(writer).await.map_err(anyhow::Error::from) };

    let on_interrupt = async {
        let _ = signals.next().await;
        let _ = term::reset();
        Ok(())
    };

    (work, on_interrupt).race().await
}
