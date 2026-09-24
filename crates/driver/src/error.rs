use std::{fmt, io, path::PathBuf};

use errors::error::HydraError;

use crate::style::{RED, RESET};

pub enum DriverError {
    Message(String),

    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },

    Diagnostics {
        errors: Vec<HydraError>,
        source: String,
        input: PathBuf,
    },
}

impl DriverError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }

    pub fn io(action: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.into(),
            source,
        }
    }

    pub fn diagnostics(errors: Vec<HydraError>, source: String, input: PathBuf) -> Self {
        Self::Diagnostics {
            errors,
            source,
            input,
        }
    }

    pub fn report(&self) {
        match self {
            Self::Diagnostics {
                errors,
                source,
                input,
            } => {
                for error in errors {
                    error.report(
                        source,
                        input.to_str().unwrap_or("<unknown>.hydra"),
                    );
                }
            }

            _ => {
                eprintln!(
                    "{}[ERROR]{} {}",
                    RED,
                    RESET,
                    self,
                );
            }
        }
    }
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => write!(f, "{}", message),

            Self::Io {
                action,
                path,
                source,
            } => {
                write!(
                    f,
                    "{} '{}': {}",
                    action,
                    path.display(),
                    source,
                )
            }

            Self::Diagnostics { .. } => write!(f, "compilation failed"),
        }
    }
}

impl fmt::Debug for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for DriverError {}
