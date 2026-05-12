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
//! motion but anything that streams Unicode characters can be used. To create a spinner, import
//! the [`Spinner`](crate::spinner::Spinner) struct and pass it a string slice:
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
//! A [`Bar`](crate::bar::Bar) renders fractional progress as a strip of characters. It is
//! defined by two characters, one for the empty portion and one for the filled portion. Create
//! one with [`Bar::new()`](crate::bar::Bar::new) or pick a pre-defined variant from
//! [`bar::styles`]:
//!
//! ```rust
//! let bar = strides::bar::styles::THIN_LINE
//!     .with_border("[", "]")
//!     .with_filled_style(owo_colors::Style::new().bright_purple());
//! ```
//!
//! Borders, an optional in-between separator, and per-portion colors are configured via the
//! builder methods on [`Bar`](crate::bar::Bar). The bar is attached to a
//! [`ProgressStyle`](crate::style::ProgressStyle) with
//! [`with_bar()`](crate::style::ProgressStyle::with_bar); width defaults to the terminal size
//! and can be overridden with
//! [`with_bar_width()`](crate::style::ProgressStyle::with_bar_width).
//!
//! ## Progress styles
//!
//! A [`ProgressStyle`](crate::style::ProgressStyle) bundles a [`Spinner`](crate::spinner::Spinner)
//! and a [`Bar`](crate::bar::Bar) into a single configuration object that can be passed to both
//! the futures and streams progress APIs:
//!
//! ```rust
//! let style = strides::style::ProgressStyle::new()
//!     .with_bar(strides::bar::styles::PARALLELOGRAM)
//!     .with_spinner(strides::spinner::styles::DOTS_3);
//! ```
//!
//! A bare [`Spinner`](crate::spinner::Spinner) can also be passed directly wherever a
//! [`ProgressStyle`](crate::style::ProgressStyle) is expected.
//!
//! ## Futures
//!
//! Import the [`FutureExt`](crate::future::FutureExt) extension trait to add the
//! [`progress()`](crate::future::FutureExt::progress) and
//! [`progress_with_messages()`](crate::future::FutureExt::progress_with_messages) methods to
//! any [`Future`]. While the future is pending, a spinner, optional progress bar and a message
//! are rendered to stdout; the line is cleared once the future resolves. `progress()` takes a
//! static message, `progress_with_messages()` takes a [`Stream`](futures_lite::Stream) whose
//! values replace the displayed message as they arrive:
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
//! .progress(DOTS_3, "this will take some time")
//! .await;
//! # });
//! ```
//!
//! For multiple concurrent futures use [`future::Group`], which renders
//! one line per task with optional per-task progress bars and dynamic messages.
//!
//! ## Streams
//!
//! Import the [`StreamExt`](crate::stream::StreamExt) extension to use the
//! [`progress()`](crate::stream::StreamExt::progress) and
//! [`progress_with_messages()`](crate::stream::StreamExt::progress_with_messages) APIs. The second
//! parameter is a closure used to calculate the progress as a fraction between 0.0 and 1.0. The
//! closure receives two parameters: the monotonically increasing item number and a reference to
//! the item itself. The former is useful if the number of stream items is known upfront and
//! determines the overall progress, whereas the second is useful to determine progress based on
//! the item itself. For example, the number of downloaded bytes.
//!
//! ```rust,no_run
//! use futures_lite::{StreamExt as _, stream};
//! use strides::stream::StreamExt;
//! use strides::style::ProgressStyle;
//! use strides::{bar, spinner};
//!
//! let style = ProgressStyle::new()
//!     .with_spinner(spinner::styles::DOTS_3)
//!     .with_bar(bar::styles::SHADED);
//!
//! # futures_lite::future::block_on(async {
//! let total = 100;
//! stream::iter(0..total)
//!     .progress(style, move |i, _| i as f64 / total as f64)
//!     .for_each(|_| {})
//!     .await;
//! # });
//! ```

pub mod bar;
pub mod future;
pub mod spinner;
pub mod stream;
pub mod style;
pub mod term;
