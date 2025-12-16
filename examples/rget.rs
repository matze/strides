use anyhow::anyhow;
use clap::Parser;
use futures::{StreamExt as _, TryStreamExt};
use strides::stream::{ProgressStyle, StreamExt as _};
use strides::{bar, spinner};
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

    let length = response.content_length().unwrap() as f64;
    let mut sum = 0;

    let progress = ProgressStyle::new()
        .with_bar(bar::styles::SHADED)
        .with_spinner(spinner::styles::DOTS_3);

    let stream = response
        .bytes_stream()
        .progress(progress, |_, item| {
            if let Ok(item) = item {
                sum += item.len();
                sum as f64 / length
            } else {
                1.0
            }
        })
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err));

    let file = tokio::fs::File::create_new(name).await?;
    let writer = FramedWrite::new(file, BytesCodec::new());

    stream.forward(writer).await?;

    Ok(())
}
