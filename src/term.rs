use crossterm::{QueueableCommand, cursor, terminal};

/// Clear the current line and move the cursor to the first column.
pub(crate) fn clear_line(stdout: &mut std::io::Stdout) -> std::io::Result<()> {
    stdout
        .queue(terminal::Clear(terminal::ClearType::CurrentLine))?
        .queue(cursor::MoveToColumn(0))?;

    Ok(())
}
