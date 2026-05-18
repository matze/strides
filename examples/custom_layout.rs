//! Showcases [`strides::layout`]: reordering segments, styling them, adding a custom segment and
//! changing the separator. Run with `cargo run --example custom_layout`.

use std::fmt::Write as _;
use std::time::Duration;

use async_io::Timer;
use futures_lite::StreamExt as _;
use owo_colors::Style;
use strides::layout::{Layout, Segment};
use strides::stream::StreamExt as _;
use strides::{bar, RenderContext, Theme};

fn percent(ctx: &RenderContext, buf: &mut String) {
    if let Some(progress) = ctx.progress {
        let _ = write!(buf, "{:>3.0}%", progress * 100.0);
    }
}

fn main() {
    futures_lite::future::block_on(async {
        let layout = Layout::from_segments([
            Segment::elapsed()
                .with_border("[", "]")
                .with_style(Style::new().green().dimmed()),
            Segment::spinner(),
            Segment::custom(percent),
            Segment::bar(),
            Segment::message(),
        ])
        .with_separator(" · ");

        let theme = Theme::default()
            .with_bar(bar::styles::THIN_LINE)
            .with_layout(layout);

        futures_lite::stream::iter(0..100)
            .progress(theme, |i, _| (i + 1) as f64 / 100.0)
            .with_elapsed_time()
            .with_label("downloading")
            .then(|item| async move {
                Timer::after(Duration::from_millis(25)).await;
                item
            })
            .for_each(|_| {})
            .await;
    })
}
