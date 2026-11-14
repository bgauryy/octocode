//! Why a streamed read fails, as the large-source code carries it up to the one
//! place that turns it into a row ([`super::window::read_failed`]).

/// Why a read of a large source is not served.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ReadError {
    /// The source's redactions must be held, and the memory budget has no room
    /// for them: none of it is served.
    Refused,
    /// Any other failure, as its message.
    Failed(String),
}

impl ReadError {
    /// The row's message.
    pub(super) fn message(&self) -> &str {
        match self {
            Self::Refused => REFUSED,
            Self::Failed(message) => message,
        }
    }
}

/// Why a read whose values the memory budget cannot place is not served.
const REFUSED: &str = "redactedContent: the memory budget has no room to hold the redactions the secret scan found in this source, so none of it is served. Split the source to read it.";

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl From<String> for ReadError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

impl From<&str> for ReadError {
    fn from(message: &str) -> Self {
        Self::Failed(message.to_owned())
    }
}
