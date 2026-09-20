use std::fmt::{self, Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    InvalidArg,
    GenericFailure,
}

#[derive(Debug)]
pub struct Error {
    pub status: Status,
    pub reason: String,
}

impl Error {
    pub fn new(status: Status, reason: impl Into<String>) -> Self {
        Self {
            status,
            reason: reason.into(),
        }
    }
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(feature = "napi-addon")]
impl From<Error> for napi::Error {
    fn from(error: Error) -> Self {
        let status = match error.status {
            Status::InvalidArg => napi::Status::InvalidArg,
            Status::GenericFailure => napi::Status::GenericFailure,
        };
        Self::new(status, error.reason)
    }
}
