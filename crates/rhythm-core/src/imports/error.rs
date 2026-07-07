use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportError {
    line: Option<usize>,
    message: String,
}

impl ImportError {
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self {
            line: None,
            message: message.into(),
        }
    }

    pub(super) fn at_line(line: usize, message: impl Into<String>) -> Self {
        Self {
            line: Some(line),
            message: message.into(),
        }
    }

    pub const fn line(&self) -> Option<usize> {
        self.line
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(formatter, "line {line}: {}", self.message),
            None => formatter.write_str(&self.message),
        }
    }
}

impl std::error::Error for ImportError {}
