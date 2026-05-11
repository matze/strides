use std::io::Write;

use crossterm::{QueueableCommand, cursor, terminal};

/// Clear the current line and move the cursor to the first column.
pub(crate) fn clear_line(stdout: &mut std::io::Stdout) -> std::io::Result<()> {
    stdout
        .queue(terminal::Clear(terminal::ClearType::CurrentLine))?
        .queue(cursor::MoveToColumn(0))?;

    Ok(())
}

/// Restore the terminal after progress rendering.
///
/// Shows the cursor (in case it was hidden by a [`Group`](crate::future::Group)) and clears
/// everything from the cursor down so multi-line group output is removed as well. Call this
/// before exiting on interrupts such as `Ctrl-C` so the terminal is left in a clean state.
pub fn reset() -> std::io::Result<()> {
    let mut stdout = std::io::stdout();
    stdout
        .queue(cursor::Show)?
        .queue(cursor::MoveToColumn(0))?
        .queue(terminal::Clear(terminal::ClearType::FromCursorDown))?;
    stdout.flush()?;
    Ok(())
}
