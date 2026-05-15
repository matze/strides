//! tokio variants of the progress wrappers.
//!
//! Mirrors the futures-io variants from [`crate::io`] but implements the tokio versions of
//! [`AsyncRead`](::tokio::io::AsyncRead) and [`AsyncWrite`](::tokio::io::AsyncWrite). The two
//! trait families are incompatible, so each ecosystem has its own wrapper type and extension
//! trait.

use std::fmt::Display;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use ::tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::state::State;
use crate::Theme;

/// Wraps a tokio [`AsyncRead`] and renders a progress line driven by the bytes read into the
/// caller-supplied [`ReadBuf`].
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

impl AsyncReadProgress<'_, ::tokio::fs::File> {
    /// Probe the underlying file's metadata and use its size as the total length.
    ///
    /// Convenience for the common case of "wrap a `tokio::fs::File` and show a bar based on its
    /// size on disk". The file's read position is unaffected.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "tokio")]
    /// # async fn run() -> std::io::Result<()> {
    /// use strides::io::tokio::AsyncReadProgressExt;
    /// use strides::spinner::styles::DOTS_3;
    ///
    /// let file = tokio::fs::File::open("data.bin").await?;
    /// let wrapped = file.progress(DOTS_3).with_file_len().await?;
    /// # let _ = wrapped;
    /// # Ok(()) }
    /// ```
    pub async fn with_file_len(self) -> io::Result<Self> {
        let len = self.inner.metadata().await?.len();
        Ok(self.with_len(len))
    }
}

impl<R> AsyncRead for AsyncReadProgress<'_, R>
where
    R: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.state.poll_spinner(cx);

        let before = buf.filled().len();
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let n = buf.filled().len() - before;
                if n == 0 {
                    this.state.finish();
                } else {
                    this.state.add_bytes(n as u64);
                    this.state.render_now();
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

/// Extension trait that adds a [`progress`](Self::progress) constructor to any tokio
/// [`AsyncRead`].
pub trait AsyncReadProgressExt: AsyncRead {
    /// Wrap this reader, returning an [`AsyncReadProgress`].
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

/// Wraps a tokio [`AsyncWrite`] and renders a progress line driven by the bytes successfully
/// written via [`poll_write`](AsyncWrite::poll_write). Useful with [`tokio::io::copy`] and the
/// download/file-copy cases that motivate it.
///
/// [`tokio::io::copy`]: ::tokio::io::copy
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

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_shutdown(cx);
        if let Poll::Ready(Ok(())) = result {
            this.state.finish();
        }
        result
    }
}

/// Extension trait that adds a [`progress`](Self::progress) constructor to any tokio
/// [`AsyncWrite`].
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
