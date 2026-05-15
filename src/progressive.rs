//! Trait surface exposed by anything renderable in a [`Group`](crate::future::Group) or
//! [`stream::Group`](crate::stream::Group).
//!
//! A [`Group`](crate::future::Group) owns and polls its work. After each poll it reads the work's
//! current state through [`Progressive`] and renders one terminal line per item. Default
//! implementations return "nothing to render" so an adapter only spells out the fields it actually
//! tracks: a future-with-spinner returns no values from any method, a byte-tracking stream returns
//! cumulative bytes and an optional total, etc.
//!
//! Built-in adapters ([`ProgressFuture`](crate::future::ProgressFuture),
//! [`ProgressStream`](crate::stream::ProgressStream),
//! [`ProgressBytesStream`](crate::stream::ProgressBytesStream)) already implement [`Progressive`];
//! user types can implement it directly to push custom work into a Group.

use std::future::Future;

use futures_lite::Stream;

/// Read-only view of an item's current progress state.
///
/// Methods return "nothing to render" by default. Adapters override only the fields they track.
/// Callers (Groups, standalone wrappers) read these on every frame and forward to the layout.
pub trait Progressive {
    /// Static label shown in the [`Label`](crate::layout::Segment::Label) segment.
    fn label(&self) -> Option<&str> {
        None
    }

    /// Dynamic message shown in the [`Message`](crate::layout::Segment::Message) segment.
    fn message(&self) -> Option<&str> {
        None
    }

    /// Progress fraction in `0.0..=1.0`, or `None` when no progress is tracked.
    fn progress(&self) -> Option<f64> {
        None
    }

    /// Cumulative bytes transferred so far.
    fn bytes_done(&self) -> u64 {
        0
    }

    /// Total bytes expected, if known.
    fn bytes_total(&self) -> Option<u64> {
        None
    }

    /// Smoothed transfer rate in bytes per second, if enough samples are available.
    fn rate(&self) -> Option<f64> {
        None
    }
}

/// A [`Future`] that also reports progress via [`Progressive`].
///
/// Blanket-implemented for any `T: Future + Progressive`. This is the trait object stored by
/// [`future::Group`](crate::future::Group).
pub trait ProgressiveFuture: Future + Progressive {}

impl<T: Future + Progressive> ProgressiveFuture for T {}

/// A [`Stream`] that also reports progress via [`Progressive`].
///
/// Blanket-implemented for any `T: Stream + Progressive`. This is the trait object stored by
/// [`stream::Group`](crate::stream::Group).
pub trait ProgressiveStream: Stream + Progressive {}

impl<T: Stream + Progressive> ProgressiveStream for T {}
