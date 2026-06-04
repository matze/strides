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
//! let layout = Layout::from_segments([
//!     Segment::spinner(),
//!     Segment::elapsed().with_border("[", "]"),
//!     Segment::bar(),
//!     Segment::message(),
//! ]);
//! ```

use std::borrow::Cow;
use std::fmt::Write as _;
use std::time::Duration;

use owo_colors::{OwoColorize as _, Style};

use crate::bar::Bar;
use crate::color::{push_gradient_chars, Gradient};

/// Values available to a [`Segment`] at render time.
///
/// Call sites fill this in once per frame; segments read from it. Fields that hold an [`Option`]
/// signal absence — the corresponding segment then renders nothing.
pub struct RenderContext<'a> {
    /// Current spinner frame, if the spinner has ticked at least once.
    pub spinner: Option<&'a str>,
    /// Number of spinner ticks so far, used to drive a pulsating gradient. Advances in lockstep
    /// with [`spinner`](Self::spinner) — no separate clock.
    pub spinner_tick: u64,
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

/// How a [`Gradient`] fills a spinner, set on a [`Spinner`](Segment::Spinner) segment via
/// [`Segment::with_gradient`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpinnerFill {
    /// Spread the gradient across the frame's cells, left to right. Best for multi-cell bands such
    /// as a scanner, whose lit cell takes on the color of wherever it currently sits.
    Cells,
    /// Pulse the whole frame through the gradient over time: every cell shares one color sampled
    /// by a triangle wave over the spinner's tick count, completing a full breathe every
    /// `2 * period` ticks. Best for single-glyph spinners, which have no spatial extent to spread
    /// across. Drawn from [`RenderContext::spinner_tick`], so it breathes in lockstep with the
    /// animation interval rather than a separate clock.
    Pulse(u32),
}

/// A single renderable element of a [`Layout`].
///
/// Construct segments with the associated functions ([`Segment::spinner`], [`Segment::elapsed`],
/// …) and refine them with the `with_*` builders. A `with_*` builder applied to a segment it does
/// not affect returns that segment unchanged.
#[derive(Clone)]
pub enum Segment {
    /// The spinner frame.
    Spinner {
        /// Explicit style; falls back to [`RenderContext::spinner_style`] when `None`. Ignored
        /// when `gradient` is set.
        style: Option<Style>,
        /// Optional gradient and how it fills the spinner, taking precedence over `style`. Lets a
        /// multi-cell spinner sweep through colors or a single-glyph one pulsate.
        gradient: Option<(Gradient, SpinnerFill)>,
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
    /// A spinner segment with no explicit style or gradient.
    pub const fn spinner() -> Self {
        Segment::Spinner {
            style: None,
            gradient: None,
        }
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
            Segment::Spinner { gradient, .. } => Segment::Spinner {
                style: Some(style),
                gradient,
            },
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

    /// Color a [`Spinner`](Segment::Spinner) segment with a [`Gradient`], filled per `fill`
    /// ([`SpinnerFill::Cells`] to spread across the frame, [`SpinnerFill::Pulse`] to breathe over
    /// time). Takes precedence over an explicit style. Other segments are returned unchanged.
    pub fn with_gradient(self, gradient: Gradient, fill: SpinnerFill) -> Self {
        match self {
            Segment::Spinner { style, .. } => Segment::Spinner {
                style,
                gradient: Some((gradient, fill)),
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
            Segment::Spinner { style, gradient } => {
                if let Some(frame) = ctx.spinner {
                    match gradient {
                        Some((gradient, SpinnerFill::Cells)) => {
                            // Spread the gradient across the frame's cells, left to right.
                            let denom = frame.chars().count().saturating_sub(1).max(1) as f64;
                            push_gradient_chars(buf, frame, |i| gradient.sample(i as f64 / denom));
                        }
                        Some((gradient, SpinnerFill::Pulse(period))) => {
                            // Sample one color by a triangle wave over the tick count and apply it
                            // uniformly, so the whole frame breathes in step with the animation.
                            let color = gradient.sample(triangle(ctx.spinner_tick, *period));
                            push_gradient_chars(buf, frame, |_| color);
                        }
                        None => {
                            let style = style.unwrap_or(ctx.spinner_style);
                            let _ = write!(buf, "{}", frame.style(style));
                        }
                    }
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

/// Triangle wave in `0.0..=1.0` over `2 * period` ticks: rises for `period` ticks, then falls back
/// for `period`. Used to drive [`SpinnerFill::Pulse`] from the spinner tick count.
fn triangle(tick: u64, period: u32) -> f64 {
    let period = period.max(1) as u64;
    let phase = tick % (2 * period);
    if phase <= period {
        phase as f64 / period as f64
    } else {
        (2 * period - phase) as f64 / period as f64
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
    Segment::Spinner {
        style: None,
        gradient: None,
    },
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

    /// Create a layout from a static slice of segments, joined by a single space. This is the
    /// only allocation-free constructor besides [`Layout::DEFAULT`]; pass a `const` slice to keep
    /// the layout backed by borrowed storage. Pass `&[]` to start empty and build up with
    /// [`with_segment`](Self::with_segment), but note that the first `with_segment` call switches
    /// the layout to an owned `Vec`.
    pub const fn new(segments: &'static [Segment]) -> Self {
        Self {
            segments: Cow::Borrowed(segments),
            separator: Cow::Borrowed(" "),
        }
    }

    /// Create a layout from any iterable of segments, collecting into a single owned `Vec`.
    /// Prefer this over `Layout::new(&[]).with_segment(a).with_segment(b)...` when the segments
    /// are listed inline: one allocation instead of one per `with_segment` call.
    pub fn from_segments<I>(segments: I) -> Self
    where
        I: IntoIterator<Item = Segment>,
    {
        Self {
            segments: Cow::Owned(segments.into_iter().collect()),
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
            spinner_tick: 0,
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

    /// Drop ANSI SGR escapes (`\x1b[...m`) so a rendered string can be compared by visible glyphs
    /// regardless of the ambient terminal's color support.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn spinner_gradient_preserves_frame_glyphs() {
        use crate::color::{Gradient, Rgb};

        let mut ctx = context();
        ctx.spinner = Some("▒▓█");
        let gradient = Gradient::new(&[(0.0, Rgb(0, 255, 0)), (1.0, Rgb(255, 0, 0))]);

        // Whatever the terminal color level, the visible glyphs are the frame, in order — for both
        // the spatial and the pulsing fill.
        let cells = render(Segment::spinner().with_gradient(gradient, SpinnerFill::Cells), &ctx);
        assert_eq!(strip_ansi(&cells), "▒▓█");

        ctx.spinner_tick = 3;
        let pulse = render(
            Segment::spinner().with_gradient(gradient, SpinnerFill::Pulse(8)),
            &ctx,
        );
        assert_eq!(strip_ansi(&pulse), "▒▓█");
    }

    #[test]
    fn triangle_wave_rises_and_falls() {
        // Over a period of 4: 0,1,2,3,4 rising to the peak, then back down to 0 at 8.
        assert_eq!(triangle(0, 4), 0.0);
        assert_eq!(triangle(2, 4), 0.5);
        assert_eq!(triangle(4, 4), 1.0);
        assert_eq!(triangle(6, 4), 0.5);
        assert_eq!(triangle(8, 4), 0.0);
        // A zero period does not divide by zero.
        assert!(triangle(5, 0).is_finite());
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
