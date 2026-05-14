# Changelog

## Unreleased

### Breaking changes

- Rename `ProgressBuilder::with_message` to `with_label` and
  `ProgressBuilder::with_fraction` to `with_progress`, so the future progress
  API uses the same vocabulary as `Task` and the `FutureExt` setters.

### Changes

- Add `Bar::render_into` to append into a caller-supplied `String` instead of
  allocating a new one, for hot paths that reuse a buffer across frames.
- Suppress all output when stdout is not a TTY.
- Lower dependency version requirements and set the minimum supported Rust
  version to 1.85.


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
