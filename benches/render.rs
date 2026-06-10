//! Benchmarks for the per-frame render path.
//!
//! The render loop is designed to be allocation-free after warm-up: one reusable buffer per line,
//! everything else appended in place. These benches guard that throughput. Note that gradient
//! output depends on the ambient terminal's detected color level (`NO_COLOR`, `COLORTERM`,
//! `TERM`), so compare runs only within the same environment.

use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use owo_colors::Style;
use strides::bar::{self, Axis, Bar};
use strides::layout::{Layout, RenderContext, Segment};
use strides::{Gradient, Rgb};

const GAUGE: Gradient = Gradient::new(&[(0.0, Rgb(0, 200, 0)), (1.0, Rgb(220, 0, 0))]);

fn context(bar: Option<&Bar>) -> RenderContext<'_> {
    RenderContext {
        spinner: Some("⠖"),
        spinner_tick: 7,
        elapsed: Duration::from_millis(1234),
        show_elapsed: true,
        bar,
        bar_width: 40,
        progress: Some(0.42),
        bytes_done: 123_456_789,
        bytes_total: Some(987_654_321),
        rate: Some(12.5 * 1024.0 * 1024.0),
        label: Some("download"),
        message: Some("chunk 17"),
        spinner_style: Style::new(),
        annotation_style: Style::new(),
    }
}

/// The default layout (spinner, elapsed, label, bar, message) over a plain shaded bar.
fn default_layout(c: &mut Criterion) {
    let layout = Layout::DEFAULT;
    let bar = bar::styles::SHADED;
    let ctx = context(Some(&bar));
    let mut buf = String::new();

    c.bench_function("default_layout", |b| {
        b.iter(|| {
            buf.clear();
            layout.render(black_box(&ctx), &mut buf);
            black_box(buf.len())
        })
    });
}

/// The byte-transfer columns (bytes, rate, ETA) that download UIs render every frame.
fn bytes_layout(c: &mut Criterion) {
    let layout = Layout::from_segments([
        Segment::spinner(),
        Segment::label(),
        Segment::bar(),
        Segment::bytes(),
        Segment::rate(),
        Segment::eta(),
    ]);
    let bar = bar::styles::THIN_LINE;
    let ctx = context(Some(&bar));
    let mut buf = String::new();

    c.bench_function("bytes_layout", |b| {
        b.iter(|| {
            buf.clear();
            layout.render(black_box(&ctx), &mut buf);
            black_box(buf.len())
        })
    });
}

/// A 40-cell bar with a per-cell gradient on the filled portion, the most escape-heavy path.
fn gradient_bar(c: &mut Criterion) {
    let bar = bar::styles::SHADED.with_filled_gradient(GAUGE, Axis::Width);
    let mut buf = String::new();

    c.bench_function("gradient_bar_40", |b| {
        b.iter(|| {
            buf.clear();
            bar.render_into(&mut buf, black_box(40), black_box(0.42));
            black_box(buf.len())
        })
    });
}

criterion_group!(benches, default_layout, bytes_layout, gradient_bar);
criterion_main!(benches);
