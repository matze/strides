//! Terminal helpers.
//!
//! [`reset()`] returns the terminal to a clean state after progress rendering. Call it from a
//! `Ctrl-C` handler when using [`Group`](crate::future::Group) so the hidden cursor is restored and
//! any leftover lines are cleared. See `examples/concurrent_futures.rs` for a complete
//! signal-handling setup.
//!
//! Rendering emits VT100/ANSI escape sequences to the stream selected by [`Output`] (stdout by
//! default). On Windows that means using Windows Terminal or PowerShell rather than legacy
//! `cmd.exe`.

use std::io::{IsTerminal, Write};

pub(crate) const HIDE_CURSOR: &[u8] = b"\x1b[?25l";
pub(crate) const SHOW_CURSOR: &[u8] = b"\x1b[?25h";

const CLEAR_CURRENT_LINE: &[u8] = b"\x1b[2K";
const MOVE_TO_COLUMN_0: &[u8] = b"\x1b[1G";
const CLEAR_FROM_CURSOR_DOWN: &[u8] = b"\x1b[J";

/// Which standard stream progress rendering is written to.
///
/// Defaults to [`Output::Stdout`]. Select it on a [`Theme`](crate::Theme) with
/// [`Theme::with_output`](crate::Theme::with_output). Render to [`Output::Stderr`] when stdout
/// carries the program's real output — for example a value captured by shell command substitution
/// — so the spinner still shows on the terminal without corrupting the captured bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Output {
    /// Write progress rendering to standard output.
    #[default]
    Stdout,
    /// Write progress rendering to standard error.
    Stderr,
}

impl Output {
    /// Whether the selected stream is connected to a terminal. Rendering is suppressed when this
    /// is false, so output redirected to a file or pipe runs to completion silently.
    pub(crate) fn is_terminal(self) -> bool {
        match self {
            Output::Stdout => std::io::stdout().is_terminal(),
            Output::Stderr => std::io::stderr().is_terminal(),
        }
    }

    /// Lock the selected stream and run `f` with the locked handle.
    pub(crate) fn with_lock<R>(self, f: impl FnOnce(&mut dyn Write) -> R) -> R {
        match self {
            Output::Stdout => f(&mut std::io::stdout().lock()),
            Output::Stderr => f(&mut std::io::stderr().lock()),
        }
    }
}

/// Clear the current line and move the cursor to the first column.
pub(crate) fn clear_line<W: Write + ?Sized>(w: &mut W) -> std::io::Result<()> {
    w.write_all(CLEAR_CURRENT_LINE)?;
    w.write_all(MOVE_TO_COLUMN_0)?;
    Ok(())
}

/// Move the cursor up `n` lines.
pub(crate) fn move_up<W: Write + ?Sized>(w: &mut W, n: u16) -> std::io::Result<()> {
    write!(w, "\x1b[{n}A")
}

/// Restores the terminal cursor when dropped, including early drops where the progress builder is
/// abandoned before completion (e.g. `break` out of a `for_each`).
pub(crate) struct CursorGuard {
    pub(crate) output: Output,
    pub(crate) is_tty: bool,
}

impl Drop for CursorGuard {
    fn drop(&mut self) {
        if self.is_tty {
            self.output.with_lock(|w| {
                let _ = clear_line(w);
                let _ = w.write_all(SHOW_CURSOR);
                let _ = w.flush();
            });
        }
    }
}

/// Restore the terminal after progress rendering on stdout.
///
/// Equivalent to [`reset_on(Output::Stdout)`](reset_on). Kept as the zero-argument entry point for
/// the common case; use [`reset_on`] when rendering to stderr.
pub fn reset() -> std::io::Result<()> {
    reset_on(Output::Stdout)
}

/// Restore the terminal after progress rendering on `output`.
///
/// Shows the cursor (in case it was hidden by a [`Group`](crate::future::Group)) and clears
/// everything from the cursor down so multi-line group output is removed as well. Call this
/// before exiting on interrupts such as `Ctrl-C` so the terminal is left in a clean state.
///
/// When the selected stream is not a terminal (e.g. redirected to a file or piped to another
/// program) this is a no-op, so signal handlers can call it unconditionally.
pub fn reset_on(output: Output) -> std::io::Result<()> {
    if !output.is_terminal() {
        return Ok(());
    }

    output.with_lock(|w| {
        w.write_all(SHOW_CURSOR)?;
        w.write_all(MOVE_TO_COLUMN_0)?;
        w.write_all(CLEAR_FROM_CURSOR_DOWN)?;
        w.flush()
    })
}
