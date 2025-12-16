/// Pre-defined progress bar styles.
pub mod styles {
    use super::Bar;

    pub const PARALLELOGRAM: Bar = Bar::new('▱', '▰');

    pub const SHADED: Bar = Bar::new('░', '█');

    pub const DOTTED: Bar = Bar::new('⣀', '⣿');
}

/// Progress bar style characters.
#[derive(Default, Clone)]
pub struct Bar<'a> {
    /// Character to symbolize incompleteness.
    empty: Option<char>,
    /// Character to symbolize completeness.
    complete: Option<char>,
    /// Characters in between complete and incomplete.
    in_between: Option<&'a str>,
    /// Left border character
    left_border: Option<&'a str>,
    /// Right border character
    right_border: Option<&'a str>,
}

impl<'a> Bar<'a> {
    pub const fn new(empty: char, complete: char) -> Self {
        Self {
            empty: Some(empty),
            complete: Some(complete),
            in_between: None,
            left_border: None,
            right_border: None,
        }
    }

    pub fn render(&self, width: usize, completed: f64) -> String {
        let completed = (completed * width as f64) as usize;
        let remaining = width.saturating_sub(completed);

        let complete = self
            .complete
            .map(|c| std::iter::repeat(c).take(completed).collect::<String>())
            .unwrap_or(String::new());

        let remaining = self
            .empty
            .map(|c| std::iter::repeat(c).take(remaining).collect::<String>())
            .unwrap_or(String::new());

        format!(
            "{}{complete}{}{remaining}{}",
            self.left_border.unwrap_or(""),
            self.in_between.unwrap_or(""),
            self.right_border.unwrap_or(""),
        )
    }

    pub fn with_in_between(mut self, chars: &'static str) -> Self {
        self.in_between = Some(chars);
        self
    }

    pub fn with_border(mut self, left: &'static str, right: &'static str) -> Self {
        self.left_border = Some(left);
        self.right_border = Some(right);
        self
    }
}
