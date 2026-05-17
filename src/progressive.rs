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
//!
//! The lifetime parameter `'a` ties any per-row [`Theme`] override to the same lifetime the
//! adapter's other borrowed data lives for. Implementations that don't supply a theme override
//! can use `impl<'a> Progressive<'a> for MyType` regardless of `'a`.

use std::future::Future;

use futures_lite::Stream;
use owo_colors::Style;

use crate::Theme;

/// Read-only view of an item's current progress state.
///
/// Methods return "nothing to render" by default. Adapters override only the fields they track.
/// Callers (Groups, standalone wrappers) read these on every frame and forward to the layout.
pub trait Progressive<'a> {
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

    /// Disable any self-rendering this value would do when polled directly.
    ///
    /// Called by [`Group::push`](crate::future::Group::push) (and the stream-side equivalent)
    /// before the first poll: Groups own the rendering for their rows, so a row that would also
    /// render standalone must be neutralised first. The default is a no-op; built-in adapters
    /// override it to drop their rendering bits.
    fn detach_rendering(&mut self) {}

    /// Per-row theme override. Returning `Some` tells the parent [`Group`](crate::future::Group)
    /// to render this row with the given theme instead of the Group's default.
    fn theme(&self) -> Option<&Theme<'a>> {
        None
    }

    /// Per-row spinner style override. `None` means "use the Group's default".
    fn spinner_style(&self) -> Option<Style> {
        None
    }

    /// Per-row annotation (label) style override. `None` means "use the Group's default".
    fn annotation_style(&self) -> Option<Style> {
        None
    }

    /// Per-row override for showing elapsed time. `None` means "use the Group's default".
    fn show_elapsed_time(&self) -> Option<bool> {
        None
    }
}

/// A [`Future`] that also reports progress via [`Progressive`].
///
/// Blanket-implemented for any `T: Future + Progressive<'a>`. This is the trait object stored by
/// [`future::Group`](crate::future::Group).
pub trait ProgressiveFuture<'a>: Future + Progressive<'a> {}

impl<'a, T: Future + Progressive<'a>> ProgressiveFuture<'a> for T {}

/// A [`Stream`] that also reports progress via [`Progressive`].
///
/// Blanket-implemented for any `T: Stream + Progressive<'a>`. This is the trait object stored by
/// [`stream::Group`](crate::stream::Group).
pub trait ProgressiveStream<'a>: Stream + Progressive<'a> {}

impl<'a, T: Stream + Progressive<'a>> ProgressiveStream<'a> for T {}
