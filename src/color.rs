//! Color gradients for bars and spinners.
//!
//! A [`Gradient`] is a list of color stops at positions in `0.0..=1.0`. Sampling it interpolates
//! between the two bracketing stops in HSL space, so a green-to-red gradient sweeps through
//! yellow and orange rather than RGB's muddy midpoint. Gradients drive per-cell coloring of the
//! filled portion of a [`Bar`](crate::bar::Bar) and, via the layout, of a spinner frame.
//!
//! ```rust
//! use strides::color::{Gradient, Rgb};
//!
//! // A classic green → red gauge.
//! const GAUGE: Gradient = Gradient::new(&[(0.0, Rgb(0, 200, 0)), (1.0, Rgb(220, 0, 0))]);
//! assert_eq!(GAUGE.sample(0.0), Rgb(0, 200, 0));
//! assert_eq!(GAUGE.sample(1.0), Rgb(220, 0, 0));
//! ```
//!
//! Output adapts to the terminal: 24-bit truecolor when `COLORTERM` advertises it, a 256-color
//! approximation otherwise, and no escape sequences at all when color is unsupported or disabled
//! via `NO_COLOR`.

use std::fmt::Write as _;
use std::sync::OnceLock;

/// An 8-bit-per-channel RGB color.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// A color gradient defined by stops at positions in `0.0..=1.0`.
///
/// Stops must be listed in ascending position order. [`sample`](Gradient::sample) interpolates
/// between the bracketing stops in HSL space and clamps positions outside the stop range to the
/// nearest endpoint. Backed by a `&'static` slice so gradients can live in `const`/`static` items
/// and stay [`Copy`] — build them as constants, mirroring [`Spinner::frames`](crate::spinner::Spinner::frames)
/// and [`Layout::new`](crate::layout::Layout::new).
#[derive(Clone, Copy)]
pub struct Gradient {
    /// Color stops, ascending by position.
    stops: &'static [(f64, Rgb)],
}

impl Gradient {
    /// Build a gradient from a static slice of `(position, color)` stops, ascending by position.
    /// `const` so gradients can live in `static`/`const` items.
    pub const fn new(stops: &'static [(f64, Rgb)]) -> Self {
        Self { stops }
    }

    /// Sample the color at `t`, clamped to `0.0..=1.0`. Positions before the first stop or after
    /// the last resolve to that endpoint's color; in between, the two bracketing stops are
    /// interpolated in HSL space. An empty gradient samples to black.
    pub fn sample(&self, t: f64) -> Rgb {
        let stops = self.stops;
        let Some(&(first_pos, first)) = stops.first() else {
            return Rgb(0, 0, 0);
        };
        let t = t.clamp(0.0, 1.0);
        if t <= first_pos {
            return first;
        }
        let &(last_pos, last) = stops.last().expect("non-empty checked above");
        if t >= last_pos {
            return last;
        }

        // Find the segment [a, b] with a.pos <= t <= b.pos and interpolate within it.
        let segment = stops
            .windows(2)
            .find(|w| t >= w[0].0 && t <= w[1].0)
            .expect("t lies between first and last stop");
        let (a_pos, a) = segment[0];
        let (b_pos, b) = segment[1];
        let span = b_pos - a_pos;
        let local = if span > 0.0 { (t - a_pos) / span } else { 0.0 };
        lerp_hsl(a, b, local)
    }
}

/// Interpolate between two colors in HSL space, taking the shortest path around the hue circle.
fn lerp_hsl(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let (h1, s1, l1) = rgb_to_hsl(a);
    let (h2, s2, l2) = rgb_to_hsl(b);
    // Shortest arc between hues, in (-180, 180].
    let dh = ((h2 - h1 + 540.0) % 360.0) - 180.0;
    let h = (h1 + t * dh).rem_euclid(360.0);
    let s = s1 + t * (s2 - s1);
    let l = l1 + t * (l2 - l1);
    hsl_to_rgb(h, s, l)
}

/// Convert RGB to HSL with hue in `[0, 360)`, saturation and lightness in `[0, 1]`.
fn rgb_to_hsl(Rgb(r, g, b): Rgb) -> (f64, f64, f64) {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s, l)
}

/// Convert HSL (hue in `[0, 360)`, saturation and lightness in `[0, 1]`) back to RGB.
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to_u8 = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb(to_u8(r1), to_u8(g1), to_u8(b1))
}

/// How much color the terminal can render, detected once from the environment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ColorLevel {
    /// No color: glyphs are written without any SGR escapes.
    None,
    /// 256-color palette: RGB is approximated to the nearest xterm-256 index.
    Ansi256,
    /// 24-bit truecolor: RGB is emitted verbatim.
    TrueColor,
}

/// The terminal's color level, detected once and cached.
fn color_level() -> ColorLevel {
    static LEVEL: OnceLock<ColorLevel> = OnceLock::new();
    *LEVEL.get_or_init(detect_color_level)
}

/// Detect the color level from `NO_COLOR`, `COLORTERM` and `TERM`.
fn detect_color_level() -> ColorLevel {
    use std::env;

    // NO_COLOR disables color when present and non-empty (https://no-color.org).
    if env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return ColorLevel::None;
    }
    if let Some(colorterm) = env::var_os("COLORTERM") {
        let colorterm = colorterm.to_string_lossy();
        if colorterm.eq_ignore_ascii_case("truecolor") || colorterm.eq_ignore_ascii_case("24bit") {
            return ColorLevel::TrueColor;
        }
    }
    match env::var_os("TERM") {
        None => ColorLevel::None,
        Some(term) if term.eq_ignore_ascii_case("dumb") => ColorLevel::None,
        Some(_) => ColorLevel::Ansi256,
    }
}

/// Approximate an RGB color with the nearest xterm-256 palette index.
fn rgb_to_ansi256(Rgb(r, g, b): Rgb) -> u8 {
    // Pure grays use the dedicated 24-step ramp (and the cube's endpoints for black/white).
    if r == g && g == b {
        return if r < 8 {
            16
        } else if r > 248 {
            231
        } else {
            232 + ((r as u16 - 8) * 24 / 247) as u8
        };
    }
    // Otherwise map each channel onto the 6×6×6 color cube.
    let cube = |v: u8| (v as f64 / 255.0 * 5.0).round() as u16;
    (16 + 36 * cube(r) + 6 * cube(g) + cube(b)) as u8
}

/// Append `n` copies of `ch` to `buf`, coloring cell `i` with `color_at(i)`.
///
/// A new SGR escape is emitted only when the color changes from the previous cell, so a coarse
/// gradient stays compact, and a single reset is written at the end. Honors the detected color
/// level: truecolor, a 256-color approximation, or no escapes at all.
pub(crate) fn push_gradient_run(
    buf: &mut String,
    ch: char,
    n: usize,
    color_at: impl Fn(usize) -> Rgb,
) {
    push_cells(buf, color_level(), (0..n).map(|i| (i, ch)), color_at);
}

/// Append the characters of `s` to `buf`, coloring char `i` with `color_at(i)`.
///
/// Like [`push_gradient_run`] but for a run whose cells differ — a multi-cell spinner frame such
/// as a comet, where each column is its own glyph rather than a repeated character.
pub(crate) fn push_gradient_chars(buf: &mut String, s: &str, color_at: impl Fn(usize) -> Rgb) {
    push_cells(buf, color_level(), s.chars().enumerate(), color_at);
}

/// Explicit-[`ColorLevel`] entry point for the repeated-char case, kept for unit tests.
#[cfg(test)]
fn push_run(
    buf: &mut String,
    ch: char,
    n: usize,
    level: ColorLevel,
    color_at: impl Fn(usize) -> Rgb,
) {
    push_cells(buf, level, (0..n).map(|i| (i, ch)), color_at);
}

/// Core writer: walk `cells` (each `(index, glyph)`), coloring by `color_at(index)`. A new SGR
/// escape is emitted only when the color changes from the previous cell, so a coarse gradient
/// stays compact, and a single reset is written at the end. Honors `level`: truecolor, a 256-color
/// approximation, or no escapes at all.
fn push_cells(
    buf: &mut String,
    level: ColorLevel,
    cells: impl Iterator<Item = (usize, char)>,
    color_at: impl Fn(usize) -> Rgb,
) {
    if level == ColorLevel::None {
        buf.extend(cells.map(|(_, ch)| ch));
        return;
    }

    let mut last: Option<Rgb> = None;
    for (i, ch) in cells {
        let color = color_at(i);
        if last != Some(color) {
            push_sgr(buf, color, level);
            last = Some(color);
        }
        buf.push(ch);
    }
    if last.is_some() {
        buf.push_str("\x1b[0m");
    }
}

/// Write the foreground SGR escape for `color` at `level`. No-op at [`ColorLevel::None`].
fn push_sgr(buf: &mut String, color: Rgb, level: ColorLevel) {
    let Rgb(r, g, b) = color;
    match level {
        ColorLevel::TrueColor => {
            let _ = write!(buf, "\x1b[38;2;{r};{g};{b}m");
        }
        ColorLevel::Ansi256 => {
            let _ = write!(buf, "\x1b[38;5;{}m", rgb_to_ansi256(color));
        }
        ColorLevel::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREEN_RED: Gradient =
        Gradient::new(&[(0.0, Rgb(0, 255, 0)), (1.0, Rgb(255, 0, 0))]);

    #[test]
    fn sample_hits_endpoints_exactly() {
        assert_eq!(GREEN_RED.sample(0.0), Rgb(0, 255, 0));
        assert_eq!(GREEN_RED.sample(1.0), Rgb(255, 0, 0));
        // Out-of-range positions clamp to the endpoints.
        assert_eq!(GREEN_RED.sample(-1.0), Rgb(0, 255, 0));
        assert_eq!(GREEN_RED.sample(2.0), Rgb(255, 0, 0));
    }

    #[test]
    fn green_red_midpoint_is_yellowish() {
        // HSL interpolation passes green → red through yellow/orange, so the midpoint hue sits
        // between green (120°) and red (0°), i.e. around yellow (60°). RGB lerp would instead give
        // a muddy (128, 128, 0).
        let mid = GREEN_RED.sample(0.5);
        let (h, _, _) = rgb_to_hsl(mid);
        assert!((30.0..=90.0).contains(&h), "midpoint hue was {h}");
        // Yellow-ish means red and green are both high.
        assert!(mid.0 > 150 && mid.1 > 150, "midpoint was {mid:?}");
    }

    #[test]
    fn empty_gradient_samples_black() {
        let empty = Gradient::new(&[]);
        assert_eq!(empty.sample(0.5), Rgb(0, 0, 0));
    }

    #[test]
    fn ansi256_maps_known_colors() {
        assert_eq!(rgb_to_ansi256(Rgb(0, 0, 0)), 16);
        assert_eq!(rgb_to_ansi256(Rgb(255, 255, 255)), 231);
        assert_eq!(rgb_to_ansi256(Rgb(255, 0, 0)), 196);
        assert_eq!(rgb_to_ansi256(Rgb(0, 255, 0)), 46);
        assert_eq!(rgb_to_ansi256(Rgb(0, 0, 255)), 21);
    }

    #[test]
    fn none_level_emits_no_escapes() {
        let mut buf = String::new();
        push_run(&mut buf, '#', 4, ColorLevel::None, |_| Rgb(10, 20, 30));
        assert_eq!(buf, "####");
        assert!(!buf.contains('\x1b'));
    }

    #[test]
    fn truecolor_emits_24bit_and_coalesces() {
        let mut buf = String::new();
        // Constant color: one SGR up front, one reset at the end.
        push_run(&mut buf, '#', 3, ColorLevel::TrueColor, |_| Rgb(1, 2, 3));
        assert_eq!(buf, "\x1b[38;2;1;2;3m###\x1b[0m");
    }

    #[test]
    fn ansi256_emits_indexed() {
        let mut buf = String::new();
        push_run(&mut buf, '#', 1, ColorLevel::Ansi256, |_| Rgb(255, 0, 0));
        assert_eq!(buf, "\x1b[38;5;196m#\x1b[0m");
    }

    #[test]
    fn distinct_colors_emit_distinct_escapes() {
        let mut buf = String::new();
        let colors = [Rgb(1, 0, 0), Rgb(2, 0, 0)];
        push_run(&mut buf, '#', 2, ColorLevel::TrueColor, |i| colors[i]);
        assert_eq!(buf, "\x1b[38;2;1;0;0m#\x1b[38;2;2;0;0m#\x1b[0m");
    }

    #[test]
    fn gradient_chars_colors_each_glyph() {
        let mut buf = String::new();
        let colors = [Rgb(1, 0, 0), Rgb(2, 0, 0)];
        push_cells(&mut buf, ColorLevel::TrueColor, "ab".chars().enumerate(), |i| {
            colors[i]
        });
        assert_eq!(buf, "\x1b[38;2;1;0;0ma\x1b[38;2;2;0;0mb\x1b[0m");
    }

    #[test]
    fn gradient_chars_none_level_is_plain() {
        let mut buf = String::new();
        push_cells(&mut buf, ColorLevel::None, "ab".chars().enumerate(), |_| {
            Rgb(1, 2, 3)
        });
        assert_eq!(buf, "ab");
    }
}
