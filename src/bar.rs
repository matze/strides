use owo_colors::OwoColorize;

/// Pre-defined progress bar styles.
pub mod styles {
    use super::Bar;

    pub const PARALLELOGRAM: Bar = Bar::new('▱', '▰');

    pub const SHADED: Bar = Bar::new('░', '█');

    pub const MEDIUM_SHADED: Bar = Bar::new('░', '▒');

    pub const HEAVY_SHADED: Bar = Bar::new('▒', '▓');

    pub const DOTTED: Bar = Bar::new('⣀', '⣿');

    pub const THIN_LINE: Bar = Bar::new('─', '━');

    pub const TRIPLE_DASH: Bar = Bar::new('┄', '┅');

    pub const MID_DOTS: Bar = Bar::new('·', '•');

    pub const EQUALS: Bar = Bar::new('╌', '═');
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
    /// Style applied to the filled portion of the bar.
    filled_style: Option<owo_colors::Style>,
    /// Style applied to the empty portion of the bar.
    empty_style: Option<owo_colors::Style>,
}

impl<'a> Bar<'a> {
    pub const fn new(empty: char, complete: char) -> Self {
        Self {
            empty: Some(empty),
            complete: Some(complete),
            in_between: None,
            left_border: None,
            right_border: None,
            filled_style: None,
            empty_style: None,
        }
    }

    pub fn render(&self, width: usize, completed: f64) -> String {
        let completed = (completed * width as f64) as usize;
        let remaining = width.saturating_sub(completed);

        let complete = self
            .complete
            .map(|c| std::iter::repeat_n(c, completed).collect::<String>())
            .unwrap_or_default();

        let remaining = self
            .empty
            .map(|c| std::iter::repeat_n(c, remaining).collect::<String>())
            .unwrap_or_default();

        let complete = match self.filled_style {
            Some(style) => complete.style(style).to_string(),
            None => complete,
        };

        let remaining = match self.empty_style {
            Some(style) => remaining.style(style).to_string(),
            None => remaining,
        };

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

    /// Style the filled portion of the bar.
    pub fn with_filled_style(mut self, style: owo_colors::Style) -> Self {
        self.filled_style = Some(style);
        self
    }

    /// Style the empty portion of the bar.
    pub fn with_empty_style(mut self, style: owo_colors::Style) -> Self {
        self.empty_style = Some(style);
        self
    }
}
