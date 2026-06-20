#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    pub length: usize,
}

impl Default for Span {
    fn default() -> Self {
        Self {
            line: 1,
            column: 1,
            length: 1
        }
    }
}

#[derive(Debug, Clone)]
pub struct HydraError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub help: Option<String>,
    pub filepath: Option<String>,
    pub source_text: Option<String>
}

impl HydraError {

    pub fn new(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            message: message.into(),
            span, 
            help: None,
            filepath: None,
            source_text: None
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_file(mut self, filepath: String, source_text: String) -> Self {
        self.filepath = Some(filepath);
        self.source_text = Some(source_text);

        self
    }

    pub fn report(&self, fallback_source: &str, fallback_filename: &str) {
        let line = self.span.line.saturating_sub(1);
        let column = self.span.column.saturating_sub(1);
        let span_length = self.span.length.max(1);

        let source = self.source_text.as_deref().unwrap_or(fallback_source);
        let filename = self.filepath.as_deref().unwrap_or(fallback_filename);

        let line_str = source.lines().nth(line).unwrap_or("");

        let padding: String = line_str.chars()
            .take(column)
            .map(|c| if c == '\t' { '\t' } else { ' ' })
            .collect();

        let red = "\x1b[31;1m";
        let blue = "\x1b[34;1m";
        let green = "\x1b[32;1m";
        let reset = "\x1b[0m";

        eprintln!("\n{}error[{}]{}\n{}", red, self.code, reset, self.message);
        eprintln!("  {}-->{} {}{}:{}:{}{}", blue, reset, blue, filename, self.span.line, self.span.column, reset);
        
        eprintln!("    {}|{}", blue, reset);
        eprintln!("{}{:>3} |{} {}", blue, self.span.line, reset, line_str);
        eprintln!("    {}| {}{}{}{}{}", blue, reset, padding, red, "^".repeat(span_length), reset);

        if let Some(help) = &self.help {
            eprintln!("    {}|{}", blue, reset);
            eprintln!("    {}={} {}help: {}{}", blue, reset, green, reset, help);
        }
    }
}
