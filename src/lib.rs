//! strides is an async-first crate to support building command line tools which display progress to
//! the user. The purpose is similar to that of the widely used indicatif crate but focuses on
//! integrating with async futures and streams and drive progress animations based on polling
//! state.
//!
//! Instead of integrating progress bar and spinner UI elements along an asynchronous program,
//! strides provides utilities to integrate these elements as part of the [`Future`] and
//! [`Stream`](futures_lite::Stream) abstractions.
//!
//! ## Spinners
//!
//! A spinner is a UI element that represents ongoing work. It is usually iconified as a circular
//! motion but anything that streams Unicode characters can be used. To create a spinner, import the
//! [`Spinner`](crate::spinner::Spinner) struct and pass it a string slice:
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
//! A [`Bar`](crate::bar::Bar) renders fractional progress as a strip of characters. It is defined
//! by two characters, one for the empty portion and one for the filled portion. Create one with
//! [`Bar::new()`](crate::bar::Bar::new) or pick a pre-defined variant from [`bar::styles`]:
//!
//! ```rust
//! let bar = strides::bar::styles::THIN_LINE
//!     .with_border("[", "]")
//!     .with_filled_style(owo_colors::Style::new().bright_purple());
//! ```
//!
//! Borders, an optional in-between separator, and per-portion colors are configured via the builder
//! methods on [`Bar`](crate::bar::Bar). The bar is attached to a [`Theme`] with
//! [`with_bar()`](crate::Theme::with_bar); width defaults to the terminal size and can be
//! overridden with [`with_bar_width()`](crate::Theme::with_bar_width).
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
//! ## Futures
//!
//! Import the [`FutureExt`](crate::future::FutureExt) extension trait and call
//! [`progress()`](crate::future::FutureExt::progress) on any [`Future`] to obtain a
//! [`ProgressBuilder`](crate::future::ProgressBuilder). Capabilities compose freely:
//! [`with_message`](crate::future::ProgressBuilder::with_message) sets a static message,
//! [`with_messages`](crate::future::ProgressBuilder::with_messages) installs a
//! [`Stream`](futures_lite::Stream) whose values replace the displayed message as they arrive, and
//! [`with_fraction`](crate::future::ProgressBuilder::with_fraction) drives the progress bar from a
//! `Stream<Item = f64>`.
//!
//! ```rust,no_run
//! use strides::future::FutureExt;
//! use strides::spinner::styles::DOTS_3;
//! use std::time::Duration;
//!
//! # futures_lite::future::block_on(async {
//! std::pin::pin!(async {
//!    // Simulate work by waiting for three seconds.
//!    futures_timer::Delay::new(Duration::from_secs(3)).await;
//! })
//! .progress(DOTS_3)
//! .with_message("this will take some time")
//! .await;
//! # });
//! ```
//!
//! For multiple concurrent futures use [`future::Group`], which renders one line per task
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
//! ```rust,no_run
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
//! ## Output
//!
//! strides currently renders all progress output to `stdout`. When `stdout` is not a terminal (for
//! example, when the program's output is redirected to a file or piped to another command) progress
//! rendering is suppressed entirely. Futures and streams still run to completion, but no spinner,
//! bar, or message bytes are written.

pub mod bar;
pub mod future;
pub mod spinner;
pub mod stream;
pub mod term;
pub mod theme;

pub use theme::Theme;
