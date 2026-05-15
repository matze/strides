//! strides is an *async-first* crate to support building command line tools which display progress
//! to the user. The purpose is similar to that of the widely used indicatif crate but focuses on
//! integrating with async futures and streams and drive progress animations based on polling state.
//! For this strides provides utilities to integrate UI elements as part of the [`Future`] and
//! [`Stream`] abstractions.
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
//! [`progress()`](crate::future::FutureExt::progress) with a [`Theme`] on any [`Future`] to animate
//! its state, which is either in-progress or done. The return value double acts as a
//! [`ProgressBuilder`](crate::future::ProgressBuilder) for further customization of the animation:
//!
//! - [`with_label`](crate::future::ProgressBuilder::with_label) sets a static label,
//! - [`with_messages`](crate::future::ProgressBuilder::with_messages) installs a
//!   [`Stream`](futures_lite::Stream) whose values replace the displayed message as they arrive,
//! - [`with_progress`](crate::future::ProgressBuilder::with_progress) drives the progress bar from a
//!   `Stream<Item = f64>`.
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
//! For multiple concurrent futures use [`Group`], which renders one line per task
//! with optional per-task progress bars and dynamic messages.
//!
//! ## Streams
//!
//! Import the [`StreamExt`](crate::stream::StreamExt) extension trait and call
//! [`progress()`](crate::stream::StreamExt::progress) with a fraction closure. The closure receives
//! the monotonically increasing item number (starting at 1) and a reference to the item, so the
//! fraction can be derived from a known total or from the item itself (e.g. accumulated bytes /
//! `Content-Length`). The returned [`StreamProgressBuilder`](crate::stream::StreamProgressBuilder)
//! accepts a [`with_messages`](crate::stream::StreamProgressBuilder::with_messages) stream for
//! dynamic text.
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
//! For byte-oriented streams use
//! [`progress_bytes()`](crate::stream::StreamExt::progress_bytes) instead. The closure returns
//! the number of bytes each item carries and the builder owns the cumulative counter, the
//! smoothed transfer rate and — when [`with_len`](crate::stream::StreamBytesProgressBuilder::with_len)
//! is set — the derived progress fraction. Pair with [`Segment::bytes`],
//! [`Segment::rate`](crate::layout::Segment::rate) and [`Segment::eta`](crate::layout::Segment::eta)
//! in a custom layout to render the byte / throughput / ETA columns that downloads typically want.
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
//! [`Group`]: crate::future::Group
//! [`Spinner`]: crate::spinner::Spinner
//! [`Bar`]: crate::bar::Bar

pub mod bar;
pub mod future;
pub mod layout;
pub mod spinner;
pub mod stream;
pub mod term;
pub mod theme;

pub(crate) mod state;

pub use layout::{Layout, RenderContext, Segment};
pub use theme::Theme;
