//! Progress wrappers for async IO traits.
//!
//! Two ecosystems are supported behind separate Cargo features:
//!
//! - The futures-io traits ([`AsyncRead`](futures_lite::AsyncRead),
//!   [`AsyncWrite`](futures_lite::AsyncWrite)) are wrapped at this module's top level when the
//!   `io` feature is enabled. The traits themselves come from `futures-lite`, which is already a
//!   dependency — the feature only gates the wrapper code.
//! - The tokio traits ([`tokio::io::AsyncRead`], [`tokio::io::AsyncWrite`]) are wrapped under
//!   [`io::tokio`](self::tokio) when the `tokio` feature is enabled. That feature additionally
//!   pulls tokio in as a runtime dependency.
//!
//! Both ecosystems use the same naming: an extension trait
//! `Async{Read,Write}ProgressExt::progress(theme)` returns a wrapper implementing the underlying
//! IO trait. The wrapper is a drop-in replacement that bumps a byte counter on every successful
//! read/write and renders a progress line.
//!
//! Pair with [`Segment::bytes`](crate::layout::Segment::bytes),
//! [`Segment::rate`](crate::layout::Segment::rate) and [`Segment::eta`](crate::layout::Segment::eta)
//! in a custom [`Layout`](crate::layout::Layout) to surface bytes / throughput / ETA columns.

#[cfg(feature = "io")]
mod futures_impl;
#[cfg(feature = "io")]
pub use futures_impl::*;

#[cfg(feature = "tokio")]
pub mod tokio;
