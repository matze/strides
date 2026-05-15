//! futures-io variants of the progress wrappers.

use std::fmt::Display;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_lite::{AsyncRead, AsyncWrite};

use crate::state::State;
use crate::Theme;

/// Wraps an [`AsyncRead`] and renders a progress line driven by the bytes that flow through
/// [`poll_read`](AsyncRead::poll_read).
///
/// Construct via [`AsyncReadProgressExt::progress`]. The wrapper is itself an [`AsyncRead`], so
/// it is a drop-in replacement that any existing `AsyncRead` consumer can accept.
pub struct AsyncReadProgress<'a, R> {
    inner: R,
    state: State<'a>,
}

impl<R> AsyncReadProgress<'_, R> {
    /// Display a static `label` while bytes flow through.
    pub fn with_label(mut self, label: impl Display) -> Self {
        self.state.set_message(label.to_string());
        self
    }

    /// Prepend `[Xs]` (seconds since the first byte was read) to the line.
    pub fn with_elapsed_time(mut self) -> Self {
        self.state.enable_elapsed_time();
        self
    }

    /// Record the total number of bytes expected. Enables the bar (via the derived fraction) and
    /// the ETA segment.
    pub fn with_len(mut self, total: u64) -> Self {
        self.state.set_bytes_total(total);
        self
    }
}

impl<R> AsyncRead for AsyncReadProgress<'_, R>
where
    R: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.state.poll_spinner(cx);

        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(0)) => {
                this.state.finish();
                Poll::Ready(Ok(0))
            }
            Poll::Ready(Ok(n)) => {
                this.state.add_bytes(n as u64);
                this.state.render_now();
                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }
}

/// Extension trait that adds a [`progress`](Self::progress) constructor to any [`AsyncRead`].
///
/// # Example
///
/// ```rust,no_run
/// # async fn run() -> std::io::Result<()> {
/// use futures_lite::io::Cursor;
/// use futures_lite::AsyncReadExt as _;
/// use strides::io::AsyncReadProgressExt;
/// use strides::spinner::styles::DOTS_3;
///
/// let data = vec![0u8; 1024];
/// let mut wrapped = Cursor::new(data).progress(DOTS_3).with_len(1024);
/// let mut sink = Vec::new();
/// wrapped.read_to_end(&mut sink).await?;
/// # Ok(()) }
/// ```
pub trait AsyncReadProgressExt: AsyncRead {
    /// Wrap this reader, returning a [`AsyncReadProgress`].
    fn progress<'a>(self, theme: impl Into<Theme<'a>>) -> AsyncReadProgress<'a, Self>
    where
        Self: Sized,
    {
        AsyncReadProgress {
            inner: self,
            state: State::new(theme.into()),
        }
    }
}

impl<R> AsyncReadProgressExt for R where R: AsyncRead {}

/// Wraps an [`AsyncWrite`] and renders a progress line driven by the bytes successfully written
/// via [`poll_write`](AsyncWrite::poll_write).
///
/// Construct via [`AsyncWriteProgressExt::progress`]. Useful with `futures_lite::io::copy` or any
/// adapter that drives writes from a reader.
pub struct AsyncWriteProgress<'a, W> {
    inner: W,
    state: State<'a>,
}

impl<W> AsyncWriteProgress<'_, W> {
    /// Display a static `label` while bytes flow through.
    pub fn with_label(mut self, label: impl Display) -> Self {
        self.state.set_message(label.to_string());
        self
    }

    /// Prepend `[Xs]` (seconds since the first byte was written) to the line.
    pub fn with_elapsed_time(mut self) -> Self {
        self.state.enable_elapsed_time();
        self
    }

    /// Record the total number of bytes expected. Enables the bar (via the derived fraction) and
    /// the ETA segment.
    pub fn with_len(mut self, total: u64) -> Self {
        self.state.set_bytes_total(total);
        self
    }
}

impl<W> AsyncWrite for AsyncWriteProgress<'_, W>
where
    W: AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.state.poll_spinner(cx);

        match Pin::new(&mut this.inner).poll_write(cx, buf) {
            Poll::Ready(Ok(n)) => {
                this.state.add_bytes(n as u64);
                this.state.render_now();
                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_close(cx);
        if let Poll::Ready(Ok(())) = result {
            this.state.finish();
        }
        result
    }
}

/// Extension trait that adds a [`progress`](Self::progress) constructor to any [`AsyncWrite`].
pub trait AsyncWriteProgressExt: AsyncWrite {
    /// Wrap this writer, returning an [`AsyncWriteProgress`].
    fn progress<'a>(self, theme: impl Into<Theme<'a>>) -> AsyncWriteProgress<'a, Self>
    where
        Self: Sized,
    {
        AsyncWriteProgress {
            inner: self,
            state: State::new(theme.into()),
        }
    }
}

impl<W> AsyncWriteProgressExt for W where W: AsyncWrite {}
