//! Terminal helpers.
//!
//! [`reset()`] returns the terminal to a clean state after progress rendering. Call it from a
//! `Ctrl-C` handler when using [`Group`](crate::future::Group) so the hidden cursor is restored and
//! any leftover lines are cleared. See `examples/concurrent_futures.rs` for a complete
//! signal-handling setup.
//!
//! Rendering emits VT100/ANSI escape sequences directly to stdout. On Windows that means using
//! Windows Terminal or PowerShell rather than legacy `cmd.exe`.

use std::io::{IsTerminal, Write};

pub(crate) const HIDE_CURSOR: &[u8] = b"\x1b[?25l";
pub(crate) const SHOW_CURSOR: &[u8] = b"\x1b[?25h";

const CLEAR_CURRENT_LINE: &[u8] = b"\x1b[2K";
const MOVE_TO_COLUMN_0: &[u8] = b"\x1b[1G";
const CLEAR_FROM_CURSOR_DOWN: &[u8] = b"\x1b[J";

/// Clear the current line and move the cursor to the first column.
pub(crate) fn clear_line<W: Write>(w: &mut W) -> std::io::Result<()> {
    w.write_all(CLEAR_CURRENT_LINE)?;
    w.write_all(MOVE_TO_COLUMN_0)?;
    Ok(())
}

/// Move the cursor up `n` lines.
pub(crate) fn move_up<W: Write>(w: &mut W, n: u16) -> std::io::Result<()> {
    write!(w, "\x1b[{n}A")
}

/// Restores the terminal cursor when dropped, including early drops where the progress builder is
/// abandoned before completion (e.g. `break` out of a `for_each`).
pub(crate) struct CursorGuard {
    pub(crate) is_tty: bool,
}

impl Drop for CursorGuard {
    fn drop(&mut self) {
        if self.is_tty {
            let mut stdout = std::io::stdout().lock();
            let _ = clear_line(&mut stdout);
            let _ = stdout.write_all(SHOW_CURSOR);
            let _ = stdout.flush();
        }
    }
}

/// Restore the terminal after progress rendering.
///
/// Shows the cursor (in case it was hidden by a [`Group`](crate::future::Group)) and clears
/// everything from the cursor down so multi-line group output is removed as well. Call this
/// before exiting on interrupts such as `Ctrl-C` so the terminal is left in a clean state.
///
/// When stdout is not a terminal (e.g. redirected to a file or piped to another program) this is
/// a no-op, so signal handlers can call it unconditionally.
pub fn reset() -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();

    if !stdout.is_terminal() {
        return Ok(());
    }

    stdout.write_all(SHOW_CURSOR)?;
    stdout.write_all(MOVE_TO_COLUMN_0)?;
    stdout.write_all(CLEAR_FROM_CURSOR_DOWN)?;
    stdout.flush()?;
    Ok(())
}
