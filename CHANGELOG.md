# Changelog

## Unreleased

### Changes

- Add back `ProgressStyle::new()` and `const`ify most constructor APIs.


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
