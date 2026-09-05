use std::fmt;

/// Simple, two failure modes.
#[derive(Debug)]
pub enum MergeError {
    Io(std::io::Error),
    Utf8 {
        position: usize,
        cause: std::str::Utf8Error,
    },
}

impl fmt::Display for MergeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergeError::Io(e) => write!(f, "{}", e),
            MergeError::Utf8 { position, cause } => {
                write!(f, "invalid utf-8 at byte {}: {}", position, cause)
            }
        }
    }
}

impl std::error::Error for MergeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MergeError::Io(e) => Some(e),
            MergeError::Utf8 { cause, .. } => Some(cause),
        }
    }
}

impl From<std::io::Error> for MergeError {
    fn from(e: std::io::Error) -> Self {
        MergeError::Io(e)
    }
}
