//! Deserializable data-transfer types mirroring the native structural-rewrite
//! engine's per-match response shape. These are pure DTOs: the parent module
//! constructs them from engine output in `run_scan` and reads them in
//! `prepare_matches`. They carry no behavior and depend on nothing beyond serde
//! and `BTreeMap`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawPosition {
    pub(super) line: u32,
    pub(super) column: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawByteRange {
    pub(super) start: usize,
    pub(super) end: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawRange {
    pub(super) byte_offset: RawByteRange,
    pub(super) start: RawPosition,
    pub(super) end: RawPosition,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct RawCapture {
    pub(super) text: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct RawMetaVariables {
    #[serde(default)]
    pub(super) single: BTreeMap<String, RawCapture>,
    #[serde(default)]
    pub(super) multi: BTreeMap<String, Vec<RawCapture>>,
    #[serde(default)]
    pub(super) transformed: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawMatch {
    pub(super) file: String,
    pub(super) text: String,
    pub(super) replacement: String,
    pub(super) range: RawRange,
    pub(super) replacement_offsets: Option<RawByteRange>,
    #[serde(default)]
    pub(super) meta_variables: RawMetaVariables,
}
