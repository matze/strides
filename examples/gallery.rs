//! Renders every predefined spinner and progress-bar style at once, one per row, each labelled
//! with its style name. Run with `cargo run --example gallery`.
//!
//! Spinners are driven through the public [`spinner`](strides::spinner) API: every style yields its
//! own [`Ticks`](strides::spinner::Ticks) stream. Progress bars are driven through
//! [`Bar::render`](strides::bar::Bar::render) against a fraction that sweeps back and forth, paced
//! by a timer. All of those event sources are merged into one stream, and the whole block is
//! repainted in place whenever anything advances. The example writes the few VT100 escapes it needs
//! (hide/show cursor, clear line, cursor up/down) itself, since those terminal helpers are internal
//! to the crate.

use std::io::Write as _;
use std::time::{Duration, Instant};

use futures_concurrency::stream::Merge as _;
use futures_lite::{future, StreamExt as _};
use strides::bar::{self, Bar};
use strides::spinner::{self, Spinner};

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
    /// The progress-bar sweep should advance one step.
    Bar,
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

    // Names are left-aligned to a shared width so spinners and bars line up in one column.
    let name_width = spinners
        .iter()
        .map(|(name, _)| name.len())
        .chain(bars.iter().map(|(name, _)| name.len()))
        .max()
        .unwrap_or(0);

    // The most recent frame for each spinner row; blank until that spinner first ticks.
    let mut frames = vec![""; spinners.len()];
    // Sweep position for the progress bars, advanced on every `Tick::Bar`.
    let mut bar_phase: u32 = 0;

    // The block is a header, the spinner rows, a blank line, a header and the bar rows.
    let total_lines = 2 + spinners.len() + 1 + bars.len();

    // Tag each spinner's frames with its row index; the bar timer contributes the sweep ticks.
    let spinner_ticks = spinners
        .iter()
        .enumerate()
        .map(|(row, (_, spinner))| spinner.ticks().map(move |frame| Tick::Spinner(row, frame)))
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

        let deadline = Instant::now() + RUN_FOR;
        let mut events = (spinner_ticks, bar_ticks).merge();

        while let Some(tick) = events.next().await {
            match tick {
                Tick::Spinner(row, frame) => frames[row] = frame,
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
