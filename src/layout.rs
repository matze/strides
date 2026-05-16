//! Composable, ordered layout for progress output.
//!
//! A [`Layout`] is an ordered list of [`Segment`]s joined by a separator. Each segment pulls its
//! value from a [`RenderContext`] supplied by the call site; segments with nothing to show
//! contribute no output and are skipped when joining, so spacing stays correct without manual
//! padding.
//!
//! [`Layout::DEFAULT`] reproduces the built-in look (spinner, elapsed time, label, bar, message).
//! Attach a custom layout to a [`Theme`](crate::Theme) with
//! [`Theme::with_layout`](crate::Theme::with_layout):
//!
//! ```rust
//! use strides::layout::{Layout, Segment};
//!
//! let layout = Layout::new(&[])
//!     .with_segment(Segment::spinner())
//!     .with_segment(Segment::elapsed().with_border("[", "]"))
//!     .with_segment(Segment::bar())
//!     .with_segment(Segment::message());
//! ```

use std::borrow::Cow;
use std::fmt::Write as _;
use std::time::Duration;

use owo_colors::{OwoColorize as _, Style};

use crate::bar::Bar;

/// Values available to a [`Segment`] at render time.
///
/// Call sites fill this in once per frame; segments read from it. Fields that hold an [`Option`]
/// signal absence — the corresponding segment then renders nothing.
pub struct RenderContext<'a> {
    /// Current spinner character, if the spinner has ticked at least once.
    pub spinner: Option<char>,
    /// Time elapsed since rendering started.
    pub elapsed: Duration,
    /// Whether the elapsed time should be rendered at all.
    pub show_elapsed: bool,
    /// Bar style used by [`Segment::Bar`].
    pub bar: &'a Bar<'a>,
    /// Bar width in characters.
    pub bar_width: usize,
    /// Current progress fraction, or `None` when no progress is tracked.
    pub progress: Option<f64>,
    /// Cumulative bytes transferred. `0` until the first byte-tracking update.
    pub bytes_done: u64,
    /// Total bytes expected, if known. Used by [`Segment::Bytes`] and [`Segment::Eta`].
    pub bytes_total: Option<u64>,
    /// Smoothed transfer rate in bytes per second, if enough samples are available.
    pub rate: Option<f64>,
    /// Static label text, if any.
    pub label: Option<&'a str>,
    /// Dynamic message text, if any.
    pub message: Option<&'a str>,
    /// Fallback style for [`Segment::Spinner`] when it carries no explicit style.
    pub spinner_style: Style,
    /// Fallback style for [`Segment::Label`] when it carries no explicit style.
    pub annotation_style: Style,
}

/// A single renderable element of a [`Layout`].
///
/// Construct segments with the associated functions ([`Segment::spinner`], [`Segment::elapsed`],
/// …) and refine them with the `with_*` builders. A `with_*` builder applied to a segment it does
/// not affect returns that segment unchanged.
#[derive(Clone)]
pub enum Segment {
    /// The spinner character.
    Spinner {
        /// Explicit style; falls back to [`RenderContext::spinner_style`] when `None`.
        style: Option<Style>,
    },
    /// Elapsed time, rendered as `1.23s` with an optional border such as `[` … `]`.
    Elapsed {
        /// Style applied to the whole token, border included.
        style: Option<Style>,
        /// Optional `(left, right)` border. `None` renders the bare value.
        border: Option<(Cow<'static, str>, Cow<'static, str>)>,
        /// Digits after the decimal point.
        precision: u8,
    },
    /// The progress bar, using the [`Bar`] and width from the [`RenderContext`].
    Bar,
    /// Static label text.
    Label {
        /// Explicit style; falls back to [`RenderContext::annotation_style`] when `None`.
        style: Option<Style>,
        /// Fixed field width in characters; text is padded or truncated to fit.
        width: Option<usize>,
    },
    /// Dynamic message text, rendered unstyled.
    Message,
    /// Cumulative bytes transferred, optionally with the known total: `1.23 MiB / 5.00 MiB` when
    /// a total is set, `1.23 MiB` otherwise. Renders nothing while no bytes have been transferred
    /// and no total is known.
    Bytes,
    /// Smoothed transfer rate, formatted as e.g. `1.23 MiB/s`. Renders nothing until enough
    /// samples are available to derive a rate.
    Rate,
    /// Estimated time remaining, derived from `bytes_total`, `bytes_done` and `rate`. Renders
    /// nothing if any of those is missing or the rate is effectively zero.
    Eta,
    /// Fixed literal text, always rendered.
    Literal(Cow<'static, str>),
    /// Arbitrary user-supplied rendering. The function appends to the buffer; appending nothing
    /// makes the segment behave as absent.
    Custom(fn(&RenderContext, &mut String)),
}

impl Segment {
    /// A spinner segment with no explicit style.
    pub const fn spinner() -> Self {
        Segment::Spinner { style: None }
    }

    /// An elapsed-time segment: no border, two digits of precision.
    pub const fn elapsed() -> Self {
        Segment::Elapsed {
            style: None,
            border: None,
            precision: 2,
        }
    }

    /// A progress-bar segment.
    pub const fn bar() -> Self {
        Segment::Bar
    }

    /// A label segment with no explicit style and no fixed width.
    pub const fn label() -> Self {
        Segment::Label {
            style: None,
            width: None,
        }
    }

    /// A message segment.
    pub const fn message() -> Self {
        Segment::Message
    }

    /// A segment showing cumulative bytes transferred, with the total when known.
    pub const fn bytes() -> Self {
        Segment::Bytes
    }

    /// A segment showing the smoothed transfer rate.
    pub const fn rate() -> Self {
        Segment::Rate
    }

    /// A segment showing the estimated time remaining.
    pub const fn eta() -> Self {
        Segment::Eta
    }

    /// A fixed literal-text segment. Accepts a `&'static str` so the constructor is `const`;
    /// callers that need an owned string can build [`Segment::Literal`] directly with
    /// `Cow::Owned`.
    pub const fn literal(text: &'static str) -> Self {
        Segment::Literal(Cow::Borrowed(text))
    }

    /// A custom segment driven by `f`.
    pub fn custom(f: fn(&RenderContext, &mut String)) -> Self {
        Segment::Custom(f)
    }

    /// Set an explicit style on a [`Spinner`](Segment::Spinner), [`Elapsed`](Segment::Elapsed) or
    /// [`Label`](Segment::Label) segment. Other segments are returned unchanged.
    pub fn with_style(self, style: Style) -> Self {
        match self {
            Segment::Spinner { .. } => Segment::Spinner { style: Some(style) },
            Segment::Elapsed {
                border, precision, ..
            } => Segment::Elapsed {
                style: Some(style),
                border,
                precision,
            },
            Segment::Label { width, .. } => Segment::Label {
                style: Some(style),
                width,
            },
            other => other,
        }
    }

    /// Wrap an [`Elapsed`](Segment::Elapsed) segment with `left` and `right` border strings, for
    /// example `"["` and `"]"`. Other segments are returned unchanged.
    pub fn with_border(
        self,
        left: impl Into<Cow<'static, str>>,
        right: impl Into<Cow<'static, str>>,
    ) -> Self {
        match self {
            Segment::Elapsed {
                style, precision, ..
            } => Segment::Elapsed {
                style,
                border: Some((left.into(), right.into())),
                precision,
            },
            other => other,
        }
    }

    /// Set the decimal precision of an [`Elapsed`](Segment::Elapsed) segment. Other segments are
    /// returned unchanged.
    pub fn with_precision(self, precision: u8) -> Self {
        match self {
            Segment::Elapsed { style, border, .. } => Segment::Elapsed {
                style,
                border,
                precision,
            },
            other => other,
        }
    }

    /// Set a fixed field width on a [`Label`](Segment::Label) segment; text is padded with spaces
    /// or truncated to fit. Other segments are returned unchanged.
    pub fn with_width(self, width: usize) -> Self {
        match self {
            Segment::Label { style, .. } => Segment::Label {
                style,
                width: Some(width),
            },
            other => other,
        }
    }

    fn render(&self, ctx: &RenderContext, buf: &mut String) {
        match self {
            Segment::Spinner { style } => {
                if let Some(ch) = ctx.spinner {
                    let style = style.unwrap_or(ctx.spinner_style);
                    let _ = write!(buf, "{}", ch.style(style));
                }
            }
            Segment::Elapsed {
                style,
                border,
                precision,
            } => {
                if !ctx.show_elapsed {
                    return;
                }
                match style {
                    Some(style) => {
                        // Styling wraps the whole token, borders included, so it has to be
                        // materialized before it can be styled.
                        let mut token = String::new();
                        if let Some((left, _)) = border {
                            token.push_str(left);
                        }
                        let _ = write!(
                            token,
                            "{:.*}s",
                            *precision as usize,
                            ctx.elapsed.as_secs_f64()
                        );
                        if let Some((_, right)) = border {
                            token.push_str(right);
                        }
                        let _ = write!(buf, "{}", token.style(*style));
                    }
                    None => {
                        if let Some((left, _)) = border {
                            buf.push_str(left);
                        }
                        let _ = write!(
                            buf,
                            "{:.*}s",
                            *precision as usize,
                            ctx.elapsed.as_secs_f64()
                        );
                        if let Some((_, right)) = border {
                            buf.push_str(right);
                        }
                    }
                }
            }
            Segment::Bar => {
                if let Some(progress) = ctx.progress {
                    ctx.bar.render_into(buf, ctx.bar_width, progress);
                }
            }
            Segment::Label { style, width } => {
                if let Some(label) = ctx.label {
                    let style = style.unwrap_or(ctx.annotation_style);
                    match width {
                        Some(width) => {
                            // Width pads to `width` chars, precision truncates to `width` chars.
                            let width = *width;
                            let _ = write!(
                                buf,
                                "{}",
                                format_args!("{label:<width$.width$}").style(style)
                            );
                        }
                        None => {
                            let _ = write!(buf, "{}", label.style(style));
                        }
                    }
                }
            }
            Segment::Message => {
                if let Some(message) = ctx.message {
                    buf.push_str(message);
                }
            }
            Segment::Bytes => {
                if ctx.bytes_done == 0 && ctx.bytes_total.is_none() {
                    return;
                }
                format_bytes_iec(ctx.bytes_done, buf);
                if let Some(total) = ctx.bytes_total {
                    buf.push_str(" / ");
                    format_bytes_iec(total, buf);
                }
            }
            Segment::Rate => {
                if let Some(rate) = ctx.rate {
                    format_bytes_iec(rate.max(0.0) as u64, buf);
                    buf.push_str("/s");
                }
            }
            Segment::Eta => {
                if let (Some(total), Some(rate)) = (ctx.bytes_total, ctx.rate) {
                    if rate > 0.0 && total > ctx.bytes_done {
                        let remaining = (total - ctx.bytes_done) as f64 / rate;
                        format_eta_secs(remaining, buf);
                    }
                }
            }
            Segment::Literal(text) => buf.push_str(text),
            Segment::Custom(f) => f(ctx, buf),
        }
    }
}

const BYTE_UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];

fn format_bytes_iec(n: u64, buf: &mut String) {
    if n < 1024 {
        let _ = write!(buf, "{n} B");
        return;
    }
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < BYTE_UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let _ = write!(buf, "{:.2} {}", value, BYTE_UNITS[unit]);
}

fn format_eta_secs(secs: f64, buf: &mut String) {
    if !secs.is_finite() || secs < 0.0 {
        return;
    }
    let total = secs as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    buf.push_str("eta ");
    if hours > 0 {
        let _ = write!(buf, "{hours}h{minutes:02}m{seconds:02}s");
    } else if minutes > 0 {
        let _ = write!(buf, "{minutes}m{seconds:02}s");
    } else {
        let _ = write!(buf, "{seconds}s");
    }
}

/// An ordered, composable description of how progress output is rendered.
///
/// A `Layout` holds a sequence of [`Segment`]s and a separator. [`render`](Layout::render) walks
/// the segments, drops the ones that produce no output, and joins the rest with the separator.
#[derive(Clone)]
pub struct Layout {
    segments: Cow<'static, [Segment]>,
    separator: Cow<'static, str>,
}

static DEFAULT_SEGMENTS: [Segment; 5] = [
    Segment::Spinner { style: None },
    Segment::Elapsed {
        style: None,
        border: None,
        precision: 2,
    },
    Segment::Label {
        style: None,
        width: None,
    },
    Segment::Bar,
    Segment::Message,
];

impl Layout {
    /// The default layout: elapsed time, spinner, label, bar and message, joined by a single
    /// space.
    pub const DEFAULT: Layout = Layout::new(&DEFAULT_SEGMENTS);

    /// Create a layout from a static slice of segments, joined by a single space. Pass `&[]` to
    /// start empty and build up with [`with_segment`](Self::with_segment).
    pub const fn new(segments: &'static [Segment]) -> Self {
        Self {
            segments: Cow::Borrowed(segments),
            separator: Cow::Borrowed(" "),
        }
    }

    /// Append `segment`, switching to owned storage.
    pub fn with_segment(mut self, segment: Segment) -> Self {
        self.segments.to_mut().push(segment);
        self
    }

    /// Set the separator inserted between non-empty segments. Defaults to a single space.
    pub fn with_separator(mut self, separator: impl Into<Cow<'static, str>>) -> Self {
        self.separator = separator.into();
        self
    }

    /// Render the layout for `ctx`, appending to `buf`.
    ///
    /// Segments that produce no output are skipped, and the separator is inserted only between
    /// segments that do produce output.
    pub fn render(&self, ctx: &RenderContext, buf: &mut String) {
        let mut first = true;

        for segment in self.segments.iter() {
            let rollback = buf.len();
            if !first {
                buf.push_str(&self.separator);
            }
            let after_separator = buf.len();

            segment.render(ctx, buf);

            // A segment that appended nothing is treated as absent: undo the separator we
            // optimistically pushed and leave `first` untouched.
            if buf.len() == after_separator {
                buf.truncate(rollback);
            } else {
                first = false;
            }
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Layout::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::bar::Bar;

    fn context() -> RenderContext<'static> {
        RenderContext {
            spinner: None,
            elapsed: Duration::from_millis(1500),
            show_elapsed: false,
            bar: EMPTY_BAR,
            bar_width: 10,
            progress: None,
            bytes_done: 0,
            bytes_total: None,
            rate: None,
            label: None,
            message: None,
            spinner_style: Style::new(),
            annotation_style: Style::new(),
        }
    }

    static EMPTY_BAR: &Bar<'static> = &Bar::empty();

    #[test]
    fn skips_empty_segments_and_their_separators() {
        let layout = Layout::new(&[])
            .with_segment(Segment::elapsed())
            .with_segment(Segment::message())
            .with_segment(Segment::bar())
            .with_segment(Segment::literal("done"));

        let mut ctx = context();
        ctx.message = Some("hello");

        let mut buf = String::new();
        layout.render(&ctx, &mut buf);

        // elapsed hidden and bar untracked → only message and literal remain.
        assert_eq!(buf, "hello done");
    }

    #[test]
    fn elapsed_border_and_precision() {
        let layout = Layout::new(&[])
            .with_segment(Segment::elapsed().with_border("[", "]").with_precision(1));

        let mut ctx = context();
        ctx.show_elapsed = true;

        let mut buf = String::new();
        layout.render(&ctx, &mut buf);

        assert_eq!(buf, "[1.5s]");
    }

    #[test]
    fn custom_separator() {
        let layout = Layout::new(&[])
            .with_segment(Segment::literal("a"))
            .with_segment(Segment::literal("b"))
            .with_separator(" | ");

        let mut buf = String::new();
        layout.render(&context(), &mut buf);

        assert_eq!(buf, "a | b");
    }

    fn render(segment: Segment, ctx: &RenderContext) -> String {
        let mut buf = String::new();
        Layout::new(&[]).with_segment(segment).render(ctx, &mut buf);
        buf
    }

    #[test]
    fn bytes_segment_skips_when_zero_and_no_total() {
        assert_eq!(render(Segment::bytes(), &context()), "");
    }

    #[test]
    fn bytes_segment_renders_done_only() {
        let mut ctx = context();
        ctx.bytes_done = 1500;
        assert_eq!(render(Segment::bytes(), &ctx), "1.46 KiB");
    }

    #[test]
    fn bytes_segment_renders_done_and_total() {
        let mut ctx = context();
        ctx.bytes_done = 1024 * 1024;
        ctx.bytes_total = Some(5 * 1024 * 1024);
        assert_eq!(render(Segment::bytes(), &ctx), "1.00 MiB / 5.00 MiB");
    }

    #[test]
    fn bytes_segment_renders_zero_when_total_known() {
        let mut ctx = context();
        ctx.bytes_total = Some(2048);
        assert_eq!(render(Segment::bytes(), &ctx), "0 B / 2.00 KiB");
    }

    #[test]
    fn rate_segment_skips_without_sample() {
        assert_eq!(render(Segment::rate(), &context()), "");
    }

    #[test]
    fn rate_segment_renders_with_unit_suffix() {
        let mut ctx = context();
        ctx.rate = Some(800.0 * 1024.0);
        assert_eq!(render(Segment::rate(), &ctx), "800.00 KiB/s");
    }

    #[test]
    fn eta_segment_skips_without_total_or_rate() {
        let mut ctx = context();
        ctx.rate = Some(1024.0);
        assert_eq!(render(Segment::eta(), &ctx), "");

        ctx.rate = None;
        ctx.bytes_total = Some(2048);
        assert_eq!(render(Segment::eta(), &ctx), "");
    }

    #[test]
    fn eta_segment_seconds() {
        let mut ctx = context();
        ctx.bytes_done = 0;
        ctx.bytes_total = Some(1024 * 50);
        ctx.rate = Some(1024.0 * 10.0);
        assert_eq!(render(Segment::eta(), &ctx), "eta 5s");
    }

    #[test]
    fn eta_segment_minutes_and_hours() {
        let mut ctx = context();
        ctx.bytes_done = 0;
        ctx.bytes_total = Some(125);
        ctx.rate = Some(1.0);
        assert_eq!(render(Segment::eta(), &ctx), "eta 2m05s");

        ctx.bytes_total = Some(3725);
        assert_eq!(render(Segment::eta(), &ctx), "eta 1h02m05s");
    }

    #[test]
    fn eta_segment_skips_when_complete() {
        let mut ctx = context();
        ctx.bytes_done = 4096;
        ctx.bytes_total = Some(4096);
        ctx.rate = Some(1024.0);
        assert_eq!(render(Segment::eta(), &ctx), "");
    }
}
