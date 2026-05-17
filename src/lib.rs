//! strides is an *async-first* crate to support building command line tools which display progress
//! to the user. The purpose is similar to that of the widely used indicatif crate but focuses on
//! integrating with async futures and streams and drive progress animations based on polling state.
//! For this strides provides utilities to integrate UI elements as part of the [`Future`] and
//! [`Stream`] abstractions.
//!
//! ## Modes of operation
//!
//! Pick a mode by how many tasks you have and how many terminal rows you want to spend on them:
//!
//! | Tasks | Rows      | Futures                                                  | Streams |
//! |-------|-----------|----------------------------------------------------------|---------|
//! | 1     | 1         | `fut.progress(theme).await`                              | `s.progress(theme, ...)` / `s.progress_bytes(theme, ...)` |
//! | N     | N         | [`future::Group::new`] + [`push`](future::Group::push) per task | [`stream::Group::new`] + [`push`](stream::Group::push) per stream |
//! | N     | 1         | [`join`](future::join())`(futs).with_theme(theme).await` | n/a |
//! | N     | 1-of-many | [`group.push(join(futs).with_label(...))`](future::join()) | n/a |
//!
//! The last row collapses many futures into a single progress line that sits alongside other rows
//! in a [`future::Group`]. Streams have no `join` collapse — push each stream as its own row.
//!
//! ## Spinners
//!
//! A spinner is a UI element that represents ongoing work. It is usually iconified as a circular
//! motion but anything that streams Unicode characters can be used. To create a spinner, import the
//! [`Spinner`] struct and pass it a string slice:
//!
//! ```rust
//! let abc = strides::spinner::Spinner::new("abc");
//! ```
//!
//! The [`ticks()`](crate::spinner::Spinner::ticks) method returns an infinite stream that cycles
//! through the characters of the string slice. The rate at which characters are cycled is set to
//! every 80ms and can be changed with the
//! [`with_interval()`](crate::spinner::Spinner::with_interval) function.
//!
//! The [`spinner::styles`] module provides a few pre-defined spinner
//! styles.
//!
//! ## Bars
//!
//! A [`Bar`] renders fractional progress as a strip of characters. It is defined by two characters,
//! one for the empty portion and one for the filled portion. In addition, optional borders,
//! in-between separator, and per-portion colors can be configured via the builder methods on
//! [`Bar`].
//!
//! Create a new bar with [`Bar::new()`](crate::bar::Bar::new) or pick a pre-defined variant from
//! [`bar::styles`]:
//!
//! ```rust
//! // This customizes the pre-defined thin line style with additional borders and colors for the
//! // filled portion.
//! let bar = strides::bar::styles::THIN_LINE
//!     .with_border("[", "]")
//!     .with_filled_style(owo_colors::Style::new().bright_purple());
//! ```
//!
//! The bar is attached to a [`Theme`] with [`with_bar()`](crate::Theme::with_bar). The bar width
//! defaults to the terminal size and can be overridden with
//! [`with_bar_width()`](crate::Theme::with_bar_width).
//!
//! ## Themes
//!
//! A [`Theme`] bundles a [`Spinner`](crate::spinner::Spinner) and a [`Bar`](crate::bar::Bar) into a
//! single configuration object that can be passed to both the futures and streams progress APIs:
//!
//! ```rust
//! let theme = strides::Theme::new()
//!     .with_bar(strides::bar::styles::PARALLELOGRAM)
//!     .with_spinner(strides::spinner::styles::DOTS_3);
//! ```
//!
//! A bare [`Spinner`](crate::spinner::Spinner) can also be passed directly wherever a
//! [`Theme`] is expected.
//!
//! ## Layout
//!
//! The order, spacing and per-element formatting of a progress line is controlled by a [`Layout`],
//! an ordered list of [`Segment`]s. [`Layout::DEFAULT`] renders elapsed time, spinner, label, bar
//! and message. Segments with nothing to show are skipped, so spacing stays correct. Attach a
//! custom layout with [`Theme::with_layout`]:
//!
//! ```rust
//! use strides::layout::{Layout, Segment};
//!
//! let theme = strides::Theme::new().with_layout(
//!     Layout::new(&[])
//!         .with_segment(Segment::spinner())
//!         .with_segment(Segment::elapsed().with_border("[", "]"))
//!         .with_segment(Segment::bar())
//!         .with_segment(Segment::message()),
//! );
//! ```
//!
//! ## Futures
//!
//! Import the [`FutureExt`](crate::future::FutureExt) extension trait and call
//! [`progress(theme)`](crate::future::FutureExt::progress) on any [`Future`] for standalone use,
//! or [`progressive()`](crate::future::FutureExt::progressive) to lift it for inclusion in a
//! [`future::Group`]. Both return a [`ProgressFuture`](crate::future::ProgressFuture) configured fluently
//! with [`with_label`](crate::future::ProgressFuture::with_label),
//! [`with_messages`](crate::future::ProgressFuture::with_messages) (a `Stream` whose values
//! replace the message), and [`with_progress`](crate::future::ProgressFuture::with_progress) (a
//! `Stream<Item = f64>` driving the bar).
//!
//! ```rust
//! use strides::future::FutureExt;
//! use strides::spinner::styles::DOTS_3;
//!
//! # futures_lite::future::block_on(
//! # async {
//! // Simulate work by waiting for three seconds.
//! futures_timer::Delay::new(std::time::Duration::from_secs(3))
//!     .progress(DOTS_3)
//!     .with_label("this will take some time")
//!     .await;
//! # }
//! # );
//! ```
//!
//! For multiple concurrent futures, push the future directly into a [`future::Group`]. The
//! [`with_label`](crate::future::FutureExt::with_label),
//! [`with_messages`](crate::future::FutureExt::with_messages),
//! [`with_progress`](crate::future::FutureExt::with_progress) and
//! [`with_elapsed_time`](crate::future::FutureExt::with_elapsed_time) setters on
//! [`FutureExt`](crate::future::FutureExt) lift the future into a
//! [`ProgressFuture`](crate::future::ProgressFuture) implicitly. Use `.progressive()` explicitly
//! when pushing a bare future with no configuration. Per-row overrides
//! ([`with_theme`](crate::future::ProgressFuture::with_theme),
//! [`with_spinner_style`](crate::future::ProgressFuture::with_spinner_style),
//! [`with_annotation_style`](crate::future::ProgressFuture::with_annotation_style)) take
//! precedence over the Group's defaults for that row. See [`future::Group`]'s docs for an
//! example.
//!
//! ## Streams
//!
//! Import the [`StreamExt`](crate::stream::StreamExt) extension trait. Standalone:
//! [`progress(theme, fraction_fn)`](crate::stream::StreamExt::progress) when each item carries
//! enough information to compute completion, or
//! [`progress_bytes(theme, bytes_fn)`](crate::stream::StreamExt::progress_bytes) for byte-oriented
//! streams (the wrapper accumulates a counter, derives a smoothed rate, and — when
//! [`with_len`](crate::stream::ProgressBytesStream::with_len) is set — derives the progress
//! fraction).
//!
//! ```rust
//! use futures_lite::{StreamExt as _, stream};
//! use strides::stream::StreamExt;
//! use strides::Theme;
//! use strides::{bar, spinner};
//!
//! let theme = Theme::new()
//!     .with_spinner(spinner::styles::DOTS_3)
//!     .with_bar(bar::styles::SHADED);
//!
//! # futures_lite::future::block_on(async {
//! let total = 100;
//! stream::iter(0..total)
//!     .progress(theme, move |i, _| i as f64 / total as f64)
//!     .for_each(|_| {})
//!     .await;
//! # });
//! ```
//!
//! For multiple concurrent streams use
//! [`progressive`](crate::stream::StreamExt::progressive) or
//! [`progressive_bytes`](crate::stream::StreamExt::progressive_bytes) and push the result into a
//! [`stream::Group`]. Pair with [`Segment::bytes`],
//! [`Segment::rate`](crate::layout::Segment::rate) and [`Segment::eta`](crate::layout::Segment::eta)
//! in a custom [`Layout`] to render the byte / throughput / ETA columns that downloads typically
//! want.
//!
//! ## Reading from `AsyncRead`
//!
//! strides does not wrap [`AsyncRead`](futures_lite::AsyncRead) directly. Convert your reader to a
//! byte stream first — for tokio,
//! [`tokio_util::io::ReaderStream`](https://docs.rs/tokio-util/latest/tokio_util/io/struct.ReaderStream.html)
//! is the canonical adapter — and feed it into
//! [`progress_bytes`](crate::stream::StreamExt::progress_bytes):
//!
//! ```rust,ignore
//! use tokio_util::io::ReaderStream;
//! use futures_lite::StreamExt as _;
//! use strides::stream::StreamExt as _;
//!
//! let mut stream = ReaderStream::new(reader)
//!     .progress_bytes(theme, |c| c.as_ref().map_or(0, |c| c.len() as u64))
//!     .with_label("download")
//!     .with_len(total);
//!
//! while let Some(chunk) = stream.next().await {
//!     writer.write_all(&chunk?).await?;
//! }
//! ```
//!
//! ## Output
//!
//! strides currently renders all progress output to `stdout`. When `stdout` is not a terminal (for
//! example, when the program's output is redirected to a file or piped to another command) progress
//! rendering is suppressed entirely. Futures and streams still run to completion, but no spinner,
//! bar, or message bytes are written.
//!
//! [`Future`]: std::future::Future
//! [`Stream`]: futures_lite::Stream
//! [`future::Group`]: crate::future::Group
//! [`stream::Group`]: crate::stream::Group
//! [`Spinner`]: crate::spinner::Spinner
//! [`Bar`]: crate::bar::Bar

pub mod bar;
pub mod future;
pub mod layout;
pub mod progressive;
pub mod spinner;
pub mod stream;
pub mod term;
pub mod theme;

pub(crate) mod line;
pub(crate) mod state;

pub use layout::{Layout, RenderContext, Segment};
pub use progressive::{Progressive, ProgressiveFuture, ProgressiveStream};
pub use theme::Theme;
