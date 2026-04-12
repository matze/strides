# Changelog

## Unreleased


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
