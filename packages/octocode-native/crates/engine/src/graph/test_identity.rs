//! Discovered test declarations; no variant represents execution.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GraphTestKind {
    Declaration,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "framework", rename_all = "lowercase")]
pub enum GraphTestIdentity {
    Googletest {
        suite: String,
        name: String,
        parameterized: bool,
        kind: GraphTestKind,
    },
    Rust {
        name: String,
        parameterized: bool,
        kind: GraphTestKind,
    },
}

impl GraphTestIdentity {
    pub fn display_name(&self) -> String {
        match self {
            Self::Googletest { suite, name, .. } => format!("{suite}.{name}"),
            Self::Rust { name, .. } => name.clone(),
        }
    }
}
