pub mod diff;
mod digest_hex;
pub mod evidence;
pub mod filesystem;
pub mod git_object;

#[cfg(feature = "napi-addon")]
mod addon;
#[cfg(feature = "napi-addon")]
pub use addon::*;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[cfg(feature = "napi-addon")]
use napi_derive::napi;

#[cfg_attr(feature = "napi-addon", napi)]
pub struct NativeCancellation {
    flag: Arc<AtomicBool>,
}

#[cfg_attr(feature = "napi-addon", napi)]
impl NativeCancellation {
    #[cfg_attr(feature = "napi-addon", napi(constructor))]
    #[must_use]
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    #[cfg_attr(feature = "napi-addon", napi)]
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }

    pub(crate) fn shared_flag(cancellation: Option<&Self>) -> Arc<AtomicBool> {
        cancellation
            .map(|value| Arc::clone(&value.flag))
            .unwrap_or_default()
    }

    #[must_use]
    pub fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.flag)
    }
}

impl Default for NativeCancellation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(feature = "napi-addon", napi(object))]
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct DiffOperation {
    pub op_type: String,
    pub line: String,
}

#[cfg_attr(feature = "napi-addon", napi(object))]
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct DiffArtifacts {
    pub diff: String,
    pub patch: String,
}
