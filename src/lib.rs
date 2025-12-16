//! strides is an async-first crate to support building command line tools which display progress to
//! the user. The purpose is similar to that of the widely used indicatif crate but focuses on
//! integrating with async futures and streams and drive progress animations based on polling
//! state.
//!
//! Instead of integrating progress bar and spinner UI elements along an asynchronous program,
//! strides provides utilities to integrate these elements as part of the [`Future`] and
//! [`Stream`](futures::Stream) abstractions.
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
//! ## Progress bars
//!
//! A progress bar is a UI element that represents the completion status of work. At the time of
//! this writing this is only applicable to (finite) streams. To create a progress bar, import the
//! [`ProgressStyle`](crate::stream::ProgressStyle) struct and attach at least a
//! [`Bar`](crate::bar::Bar) to style the progress bar:
//!
//! ```rust
//! let style = strides::stream::ProgressStyle::new()
//!     .with_bar(strides::bar::styles::PARALLELOGRAM);
//! ```
//!
//! Then import the [`StreamExt`](crate::stream::StreamExt) extension to use the
//! [`progress()`](crate::stream::StreamExt::progress) and
//! [`progress_with_messages()`](crate::stream::StreamExt::progress_with_messages) APIs. The second
//! parameter is a closure used to calculate the progress as a fraction between 0.0 and 1.0. The
//! closure receives two parameters: the monotonically increasing item number and at reference to
//! the item itself. The former is useful if the number of stream items is known upfront and
//! determines the overall progress, whereas the second is useful to determine progress based on
//! the item itself. For example, the number of downloaded bytes.
//!
//! ## Examples
//!
//! ### Single future is in progress
//!
//! In the simplest case, you can use strides to display that a [`Future`] has not completed yet. For
//! that import the [`FutureExt`](crate::future::FutureExt) extension trait that adds the
//! [`progress()`](crate::future::FutureExt::progress) method to futures which shows a spinner and
//! a message:
//!
//! ```rust,no_run
//! use strides::future::FutureExt;
//! use strides::spinner::styles::DOTS_3;
//! use std::time::Duration;
//!
//! # futures_lite::future::block_on(async {
//! std::pin::pin!(async {
//!    // Simulate work by waiting for three seconds.
//!    async_io::Timer::after(Duration::from_secs(3)).await;
//! })
//! .progress(DOTS_3, "this will take some time")
//! .await;
//! # });
//! ```

pub mod bar;
pub mod future;
pub mod spinner;
pub mod stream;

mod term;
