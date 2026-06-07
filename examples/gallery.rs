//! Renders every predefined spinner and progress-bar style at once, one per row, each labelled
//! with its style name. Run with `cargo run --example gallery`.
//!
//! Spinners are driven through the public [`spinner`](strides::spinner) API: every style yields its
//! own [`Ticks`](strides::spinner::Ticks) stream. Progress bars are driven through
//! [`Bar::render`](strides::bar::Bar::render) against a fraction that sweeps back and forth, paced
//! by a timer. Gradient spinners go through the real layout path —
//! [`Segment::spinner().with_gradient(..)`](strides::layout::Segment::with_gradient) rendered via a
//! [`RenderContext`](strides::layout::RenderContext). All of those event sources are merged into one
//! stream, and the whole block is repainted in place whenever anything advances. The example writes
//! the few VT100 escapes it needs (hide/show cursor, clear line, cursor up/down) itself, since those
//! terminal helpers are internal to the crate.

use std::io::Write as _;
use std::time::{Duration, Instant};

use futures_concurrency::stream::Merge as _;
use futures_lite::{future, StreamExt as _};
use owo_colors::Style;
use strides::bar::{self, Axis, Bar};
use strides::color::{Gradient, Rgb};
use strides::layout::{Layout, RenderContext, Segment, SpinnerFill};
use strides::spinner::{self, Spinner};

/// Green→red ramp, interpolated through yellow/orange in HSL space.
const GREEN_RED: Gradient = Gradient::new(&[(0.0, Rgb(0, 200, 0)), (1.0, Rgb(220, 0, 0))]);

/// One hue breathing from dim to bright — a lightness pulse for a twinkling star. Both stops are
/// the same blue, so the pulse varies brightness rather than cycling color.
const STAR_PULSE: Gradient = Gradient::new(&[(0.0, Rgb(40, 50, 90)), (1.0, Rgb(180, 210, 255))]);

/// Warm orange shimmer: dim base with a bright spotlight sweeping across the text. The gradient
/// maps t=0 (far from the spotlight) to dim orange, and t=1 (at the spotlight center) to bright orange.
const ORANGE_SHIMMER: Gradient = Gradient::new(&[(0.0, Rgb(180, 90, 0)), (1.0, Rgb(255, 180, 50))]);

/// A status word that shimmers in place: a single, unchanging frame, so nothing animates but the
/// bright spotlight that [`SpinnerFill::Sweep`] washes across the letters, left to right and back.
/// Pair it with [`ORANGE_SHIMMER`] for the "vibeing…" look — the whole word orange, individual
/// letters lifting brighter as the highlight passes.
const VIBING: Spinner = Spinner::frames(&["vibeing…"]);

/// Purple breathing from dim to bright. Both stops share a hue, so [`SpinnerFill::Pulse`] varies
/// the whole word's brightness rather than cycling color.
const PURPLE_PULSE: Gradient = Gradient::new(&[(0.0, Rgb(70, 40, 110)), (1.0, Rgb(190, 130, 255))]);

/// Like [`VIBING`], a single unchanging frame, but driven by [`SpinnerFill::Pulse`]: every letter
/// shares one color, so the whole "spinning…" word breathes brighter and dimmer in unison.
const SPINNING: Spinner = Spinner::frames(&["spinning…"]);

/// How long the gallery animates before restoring the terminal and exiting.
const RUN_FOR: Duration = Duration::from_secs(8);

/// Character width of the rendered progress bars.
const BAR_WIDTH: usize = 24;

/// Timer period driving the progress-bar sweep.
const BAR_PERIOD: Duration = Duration::from_millis(80);

/// Steps in one direction of the back-and-forth bar sweep.
const BAR_STEPS: u32 = 24;

/// One unit of work surfaced by the merged event stream.
enum Tick {
    /// Spinner on `row` advanced to `frame`.
    Spinner(usize, &'static str),
    /// Gradient spinner on `row` advanced to `frame`.
    GradientSpinner(usize, &'static str),
    /// The progress-bar sweep should advance one step.
    Bar,
}

/// Render a single spinner `frame` colored by `gradient` and filled per `fill`, going through the
/// real layout path so the example exercises [`Segment::with_gradient`] rather than reimplementing
/// coloring. `tick` drives the pulse for [`SpinnerFill::Pulse`] and is ignored otherwise.
fn gradient_spinner(
    gradient: Gradient,
    fill: SpinnerFill,
    frame: &str,
    tick: u64,
    bar: &Bar,
) -> String {
    let layout = Layout::from_segments([Segment::spinner().with_gradient(gradient, fill)]);
    let ctx = RenderContext {
        spinner: Some(frame),
        spinner_tick: tick,
        elapsed: Duration::ZERO,
        show_elapsed: false,
        bar,
        bar_width: 0,
        progress: None,
        bytes_done: 0,
        bytes_total: None,
        rate: None,
        label: None,
        message: None,
        spinner_style: Style::new(),
        annotation_style: Style::new(),
    };
    let mut buf = String::new();
    layout.render(&ctx, &mut buf);
    buf
}

fn main() {
    // Every predefined spinner style paired with the name it is exported under.
    let spinners: &[(&str, Spinner)] = &[
        ("ARC", spinner::styles::ARC),
        ("DOTS", spinner::styles::DOTS),
        ("DOTS_2", spinner::styles::DOTS_2),
        ("DOTS_3", spinner::styles::DOTS_3),
        ("DOTS_4", spinner::styles::DOTS_4),
        ("DOTS_5", spinner::styles::DOTS_5),
        ("DOTS_6", spinner::styles::DOTS_6),
        ("DOTS_7", spinner::styles::DOTS_7),
        ("DOTS_8", spinner::styles::DOTS_8),
        ("DOTS_CIRCLE", spinner::styles::DOTS_CIRCLE),
        ("DOT_LARGE_SQUARE", spinner::styles::DOT_LARGE_SQUARE),
        ("STAR", spinner::styles::STAR),
        ("SAND", spinner::styles::SAND),
        ("KNIGHT", spinner::styles::KNIGHT),
        ("KNIGHT_COMET", spinner::styles::KNIGHT_COMET),
        ("BOUNCE", spinner::styles::BOUNCE),
        ("BAR", spinner::styles::BAR),
        ("PULSE", spinner::styles::PULSE),
    ];

    // Every predefined progress-bar style paired with its exported name.
    let bars: &[(&str, Bar)] = &[
        ("PARALLELOGRAM", bar::styles::PARALLELOGRAM),
        ("SHADED", bar::styles::SHADED),
        ("MEDIUM_SHADED", bar::styles::MEDIUM_SHADED),
        ("HEAVY_SHADED", bar::styles::HEAVY_SHADED),
        ("DOTTED", bar::styles::DOTTED),
        ("THIN_LINE", bar::styles::THIN_LINE),
        ("TRIPLE_DASH", bar::styles::TRIPLE_DASH),
        ("MID_DOTS", bar::styles::MID_DOTS),
        ("EQUALS", bar::styles::EQUALS),
    ];

    // The same SHADED bar colored by a green→red gradient, one row per mapping axis.
    let gradient_bars: &[(&str, Bar)] = &[
        (
            "WIDTH",
            bar::styles::SHADED.with_filled_gradient(GREEN_RED, Axis::Width),
        ),
        (
            "FRACTION",
            bar::styles::SHADED.with_filled_gradient(GREEN_RED, Axis::Fraction),
        ),
    ];

    // Gradient-colored spinners. `Cells` spreads the gradient across a multi-cell band, so the
    // KNIGHT scanner changes hue as it sweeps. `Pulse` breathes one color over time, sampled by the
    // spinner's tick count: STAR twinkles in place — no spatial motion — so the lightness pulse
    // reads as the whole glyph brightening and dimming. `Sweep` moves a bright spotlight across the
    // frame: the VIBING word never changes, so only the highlight travels, lifting individual
    // letters brighter from left to right and back — the "vibeing…" shimmer.
    let gradient_spinners: &[(&str, Spinner, Gradient, SpinnerFill)] = &[
        (
            "KNIGHT (Cells)",
            spinner::styles::KNIGHT,
            GREEN_RED,
            SpinnerFill::Cells,
        ),
        (
            "STAR (Pulse)",
            spinner::styles::STAR,
            STAR_PULSE,
            SpinnerFill::Pulse(8),
        ),
        (
            "spinning… (Pulse)",
            SPINNING,
            PURPLE_PULSE,
            SpinnerFill::Pulse(12),
        ),
        (
            "vibeing… (Sweep)",
            VIBING,
            ORANGE_SHIMMER,
            SpinnerFill::Sweep(12),
        ),
    ];

    // Names are left-aligned to a shared width so every row lines up in one column.
    let name_width = spinners
        .iter()
        .map(|(name, _)| name.len())
        .chain(bars.iter().map(|(name, _)| name.len()))
        .chain(gradient_bars.iter().map(|(name, _)| name.len()))
        .chain(gradient_spinners.iter().map(|(name, _, _, _)| name.len()))
        .max()
        .unwrap_or(0);

    // The most recent frame for each (plain / gradient) spinner row; blank until it first ticks.
    let mut frames = vec![""; spinners.len()];
    let mut gradient_frames = vec![""; gradient_spinners.len()];
    // Per-row tick count for the gradient spinners, advanced on each tick to drive the pulse.
    let mut gradient_ticks = vec![0u64; gradient_spinners.len()];
    // Sweep position for the progress bars, advanced on every `Tick::Bar`.
    let mut bar_phase: u32 = 0;

    // Each section is a header plus its rows; sections after the first add a leading blank line.
    let total_lines =
        1 + spinners.len() + 2 + bars.len() + 2 + gradient_bars.len() + 2 + gradient_spinners.len();

    // Tag each spinner's frames with its row index; the bar timer contributes the sweep ticks.
    let spinner_ticks = spinners
        .iter()
        .enumerate()
        .map(|(row, (_, spinner))| spinner.ticks().map(move |frame| Tick::Spinner(row, frame)))
        .collect::<Vec<_>>()
        .merge();
    let gradient_spinner_ticks = gradient_spinners
        .iter()
        .enumerate()
        .map(|(row, (_, spinner, _, _))| {
            spinner
                .ticks()
                .map(move |frame| Tick::GradientSpinner(row, frame))
        })
        .collect::<Vec<_>>()
        .merge();
    let bar_ticks = async_io::Timer::interval(BAR_PERIOD).map(|_| Tick::Bar);

    future::block_on(async move {
        // Hide the cursor and reserve one line per row, then return to the top of the block.
        print!("\x1b[?25l");
        for _ in 0..total_lines {
            println!();
        }
        print!("\x1b[{total_lines}A");
        let _ = std::io::stdout().flush();

        // A throwaway bar for the RenderContext used to color gradient spinners; the spinner-only
        // layout never renders it.
        let demo_bar = Bar::new(' ', ' ');

        let deadline = Instant::now() + RUN_FOR;
        let mut events = (spinner_ticks, gradient_spinner_ticks, bar_ticks).merge();

        while let Some(tick) = events.next().await {
            match tick {
                Tick::Spinner(row, frame) => frames[row] = frame,
                Tick::GradientSpinner(row, frame) => {
                    gradient_frames[row] = frame;
                    gradient_ticks[row] += 1;
                }
                Tick::Bar => bar_phase += 1,
            }

            // Triangle wave in `0.0..=1.0`: fill up, then drain back down.
            let step = bar_phase % (2 * BAR_STEPS);
            let completed = if step <= BAR_STEPS {
                step as f64 / BAR_STEPS as f64
            } else {
                (2 * BAR_STEPS - step) as f64 / BAR_STEPS as f64
            };

            // Repaint the whole block in place.
            let mut out = std::io::stdout().lock();
            let mut line = |content: &str| {
                let _ = writeln!(out, "\x1b[2K\x1b[1G{content}");
            };
            line("\x1b[1mSpinners\x1b[0m");
            for (row, (name, _)) in spinners.iter().enumerate() {
                line(&format!("  {name:<name_width$}  {}", frames[row]));
            }
            line("");
            line("\x1b[1mProgress bars\x1b[0m");
            for (name, bar) in bars {
                line(&format!(
                    "  {name:<name_width$}  {}",
                    bar.render(BAR_WIDTH, completed)
                ));
            }
            line("");
            line("\x1b[1mGradient spinners\x1b[0m");
            for (row, (name, _, gradient, fill)) in gradient_spinners.iter().enumerate() {
                line(&format!(
                    "  {name:<name_width$}  {}",
                    gradient_spinner(
                        *gradient,
                        *fill,
                        gradient_frames[row],
                        gradient_ticks[row],
                        &demo_bar
                    )
                ));
            }
            line("");
            line("\x1b[1mGradient bars\x1b[0m");
            for (name, bar) in gradient_bars {
                line(&format!(
                    "  {name:<name_width$}  {}",
                    bar.render(BAR_WIDTH, completed)
                ));
            }
            let _ = write!(out, "\x1b[{total_lines}A");
            let _ = out.flush();

            if Instant::now() >= deadline {
                break;
            }
        }

        // Drop below the block and restore the cursor.
        print!("\x1b[{total_lines}B\x1b[?25h");
        let _ = std::io::stdout().flush();
    });
}
