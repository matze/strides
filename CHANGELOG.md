# Changelog

## Unreleased

### Breaking changes

- A spinner frame is now a string slice rather than a single character, so
  `Spinner::ticks()` yields `&str` instead of `char`. `Spinner::new("◜◝◞◟")`
  still splits a string into per-character frames, so single-glyph spinners are
  unchanged; only code that consumed the `Ticks` stream's items directly needs
  to adapt.
- `RenderContext` gained a `spinner_tick: u64` field (the spinner's tick count,
  driving pulsating gradients), and the `Segment::Spinner` variant gained a
  `gradient` field. Code that constructs `RenderContext` or matches/builds
  `Segment::Spinner` directly needs to add these; prefer the `Segment::spinner()`
  constructor and the `with_*` builders, which are unaffected.

### Added

- `Spinner::frames` builds a spinner from explicit multi-character frames, one
  `&str` per frame, for animations that span several columns. New multi-cell
  styles `KNIGHT` (Knight Rider / K.I.T.T. scanner), `KNIGHT_COMET`, `BOUNCE`,
  `BAR` and `PULSE` are built on it. See the `gallery` example for a live demo of
  every spinner and progress-bar style.
- A `color` module with `Rgb` and `Gradient` (re-exported at the crate root).
  `Gradient::new(&[(pos, Rgb)])` defines color stops interpolated in HSL space.
  Output adapts to the terminal: 24-bit truecolor when `COLORTERM` advertises
  it, a 256-color approximation otherwise, and no escapes at all when color is
  unsupported or disabled via `NO_COLOR`.
- `Bar::with_filled_gradient` / `with_empty_gradient` color a bar per cell from a
  `Gradient`, mapped by a `bar::Axis`: `Width` (fixed to column, revealed as the
  bar fills) or `Fraction` (one color keyed to the fill level, a gauge).
- `Segment::with_gradient(gradient, fill)` colors a spinner, with `SpinnerFill`
  selecting `Cells` (spread across a multi-cell band, so a scanner changes hue as
  it sweeps) or `Pulse(period)` (one color breathing over the spinner's tick
  count, for single-glyph spinners). The `gallery` example demos both, plus all
  three bar axes.

## 1.0.0-rc.3

### Breaking changes

- The fraction closure passed to `StreamExt::progress` / `progressive` now
  receives a zero-based item index, matching `Iterator::enumerate`. Previously
  the index was incremented before the closure call, so the first item saw `1`
  and a `|i, _| i as f64 / N as f64` closure reached `1.0` on item `N`. Update
  such closures to `(i + 1) as f64 / N as f64` to keep the old shape, or
  migrate to `StreamExt::progress_count` (see below).
- `Theme::default()` now ships the `spinner::styles::DOTS_3` spinner instead of
  the inactive one, so `fut.progress(Theme::default()).await` renders a visible
  animation out of the box. Use `Theme::new()` for the previous empty-theme
  semantics when building a theme bottom-up.
- `with_messages` (on `ProgressFuture`, `ProgressStream`, `ProgressBytesStream`,
  `ProgressCountStream`, `Join` and `FutureExt`) now bounds the stream item by
  `Into<Cow<'static, str>>` instead of `Display`. `&'static str` and `String`
  pass through with no allocation (previously every yielded message was
  `to_string()`'d into a fresh `String`). Callers passing other `Display` types
  (integers, custom formatters) now need to `format!` at the call site.
- `Progressive::show_elapsed_time` now returns `bool` (default `false`) instead
  of `Option<bool>`. A row opts in by returning `true`; a `Group` ORs that with
  its own `with_elapsed_time` default. The previous `Some(false)` state was
  unreachable — rows could only opt in — so manual implementors return `true`
  where they used to return `Some(true)` and drop the `None` arm.

### Changes

- Add `StreamExt::progress_count` / `progressive_count` and the
  `ProgressCountStream` adapter: items are counted internally and the bar
  fraction is derived from a known total. The total is seeded from
  `Stream::size_hint` so bounded sources like `iter(Vec)` and `iter(0..n)`
  render a filled bar with no extra ceremony; `with_len(n)` overrides when the
  hint is missing or inaccurate.
- Add `Theme::with(spinner, bar)` for the common case of naming both pieces in
  one call: `Theme::with(DOTS_3, SHADED)` instead of
  `Theme::new().with_spinner(DOTS_3).with_bar(SHADED)`.
- Add `Layout::from_segments(impl IntoIterator<Item = Segment>)` for bulk
  construction in a single allocation instead of chaining `with_segment` per
  segment. `Layout::new(&'static [Segment])` remains the only allocation-free
  constructor besides `Layout::DEFAULT`.


## 1.0.0-rc.2

### Breaking changes

- Introduce the `Progressive` trait (with `ProgressiveFuture` and
  `ProgressiveStream` supertraits) as the rendering contract. Anything pushed
  into a `Group` implements it; user-defined work can implement it directly.
- Rename adapter types:
  - `future::ProgressBuilder` → `future::ProgressFuture`
  - `stream::StreamProgressBuilder` → `stream::ProgressStream`
  - `stream::StreamBytesProgressBuilder` → `stream::ProgressBytesStream`
- `future::Group<'a, F>` becomes `future::Group<'a, O>` parameterised on the
  output type instead of the future type. Heterogeneous push is the default
  (boxed `dyn ProgressiveFuture<Output = O>`).
- `future::Task` is removed. Push futures directly; the
  [`FutureExt`](crate::future::FutureExt) setters
  (`with_label`/`with_messages`/`with_progress`/`with_elapsed_time`) implicitly
  lift a bare future into a tracked-only `ProgressFuture`, so
  `group.push(fut.with_label("x"))` works without spelling out `.progressive()`.
  Use `.progressive()` explicitly when pushing a bare future with no
  configuration.
- Add `stream::Group<'a, I>` for rendering multiple concurrent streams as one
  line per stream, with `StreamExt::progressive` and `progressive_bytes` as
  constructors for the tracked form.

### Fixed

- `future::Group` no longer panics with "async fn resumed after completion"
  when several pushed futures become `Ready` in the same `poll_next` call. The
  previous loop captured only the first completion and left other completed
  slots populated, so the next poll re-polled an already-finished future.
  Outputs are now buffered (matching `stream::Group`) and drained one per
  `poll_next`. As a result `Stream for future::Group<'_, O>` now requires
  `O: Unpin` (consistent with `stream::Group`).


## 1.0.0-rc.1

### Breaking changes

- Rename `ProgressBuilder::with_message` to `with_label` and
  `ProgressBuilder::with_fraction` to `with_progress`, so the future progress
  API uses the same vocabulary as `Task` and the `FutureExt` setters.
- `Group::with_elapsed_time` no longer takes a `bool`. It is off by default and
  the no-argument builder method enables it.
- The progress line layout is now unified across futures, streams and `Group`.
  `Group`'s segment order changes (previously `spinner [elapsed] label bar
  message`), and `with_elapsed_time` renders the elapsed time as `Xs` instead of
  `[Xs]`. Restore borders with a custom `Layout` containing
  `Segment::elapsed().with_border("[", "]")`.

### Changes

- Add the `layout` module with `Layout`, `Segment` and `RenderContext` for
  composable, type-safe control over progress-line segment order, spacing,
  separators and per-segment formatting. Used with `Theme::with_layout`.
- Add `StreamProgressBuilder::with_label` to display a static label, mirroring
  `ProgressBuilder::with_label` on the future side.
- Add `with_elapsed_time` to `ProgressBuilder` and `StreamProgressBuilder` to
  prepend `[Xs]` elapsed time to the line, mirroring `Group`.
- Add `Bar::render_into` to append into a caller-supplied `String` instead of
  allocating a new one, for hot paths that reuse a buffer across frames.
- Suppress all output when stdout is not a TTY.
- Lower dependency version requirements and set the minimum supported Rust
  version to 1.85.

### Fixed

- Hide the terminal cursor while a `ProgressBuilder` or `StreamProgressBuilder`
  renders and restore it when the future/stream completes or the builder is
  dropped early, so it no longer blinks at the end of the progress line.


## 0.4.0

### Breaking changes

- Rename `style::ProgressStyle` to `Theme` and re-export it at the crate
  root as `strides::Theme`. The `style` module is renamed to `theme`. The
  legacy `stream::ProgressStyle` re-export is dropped.
- Reshape the progress API around builders so capabilities compose
  orthogonally:
  - `FutureExt::progress(theme)` now returns a `ProgressBuilder` with
    `with_message`, `with_messages`, and `with_fraction` setters. The previous
    `progress(theme, message)` and `progress_with_messages(...)` methods are
    removed.
  - `StreamExt::progress(theme, fraction_fn)` now returns a
    `StreamProgressBuilder` with `with_messages`. The previous
    `progress_with_messages(...)` method is removed.
  - `Group::push(...)` accepts `impl Into<Task<'_, F>>`. `push_with_messages`
    and `push_with_progress` are removed and configured via the new public
    `future::Task` type or the mirrored `FutureExt::with_label`,
    `FutureExt::with_messages`, and `FutureExt::with_progress` methods. The
    label is rendered between the elapsed-time block and the progress bar
    (previously the per-task `prefix` came after the bar).
- `StreamExt::progress` now accepts `impl Into<Theme<'a>>`, so a bare `Spinner`
  can be passed directly (mirroring `FutureExt::progress`).

### Changes

- Add back `Theme::new()` and `const`ify most constructor APIs.
- Tasks pushed into a `Group` with no message or progress stream no longer
  allocate `Box<dyn Stream>` placeholders.
- `Group` tasks accept `impl Display` for prefix and message stream items,
  matching `FutureExt`.


## 0.3.0

### Breaking changes

- Replace `ProgressStyle::new()` with `Default` implementation.
- `future::Group::new()` now accepts `impl Into<ProgressStyle>` instead
  of a bare `Spinner`. Existing call sites continue to compile thanks
  to the `From<Spinner>` impl.

### Changes

- Pull in `futures-util` instead of all of `futures`.

### Added

- Added `Bar::with_filled_style()` and `Bar::with_empty_style()` to style the
  bar elements.
- Additional `Bar` styles.
- `Bar::with_filled_style()` and `Bar::with_empty_style()` to style the bar
  elements.
- `future::Group::push_with_progress()` renders a per-task progress bar from a
  `Stream<Item = f64>`. The bar style and width are taken from the
  `ProgressStyle` passed to `Group::new()`.
- `strides::term::reset()` to reset the output.


## 0.2.0

### Breaking changes

- Renamed `future::Monitored` to `future::Group`.

### Changed

- Allow `future::Group` to assign dynamic messages.
- Avoid overdraw via dirty-tracking.
- Use `futures-timer` instead of `async-io` timer.

### Fixed

- Stale last line spinner in a `future::Monitored`.


## 0.1.0

### Breaking changes

- `ProgressStyle` moved from `stream` module to new `style` module. A
  re-export in `stream` preserves the old import path.
- `Spinner::ticks()` now returns a concrete `Ticks<'a>` type instead of
  `impl Stream<Item = char>`.
- `Monitored::new()` accepts a `Spinner` directly instead of a raw tick
  stream. Callers no longer need to call `.ticks()` manually.
- `FutureExt::progress()` now accepts `impl Into<ProgressStyle>` instead
  of a bare `Spinner`. Existing call sites continue to compile thanks to
  the `From<Spinner>` impl.
- Spinner style `CIRCLE` renamed to `ARC`.
- Spinner style constants `DOTS_7` and `DOT_LARGE_SQUARE` changed from
  `&str` to `Spinner`.

### Added

- `FutureExt::progress_with_messages()` for dynamic messages on futures,
  mirroring the existing stream API.
- `ProgressStyle::with_bar_width()` to configure bar width. Defaults to
  terminal width detection, falling back to 40 characters.
- Futures now render a progress bar when one is configured via
  `ProgressStyle`.
- New spinner styles: `DOTS`, `DOTS_2`, `DOTS_4`, `DOTS_5`, `DOTS_6`,
  `DOTS_8`, `DOTS_CIRCLE`, and `STAR`.

### Fixed

- Last message no longer disappears when the messages stream is
  exhausted before the future or stream completes.


## 0.0.0

Initial release.
