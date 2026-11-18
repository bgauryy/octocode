//! Integer defaults and bounds of each tool's query schema (`FIELD_KEYWORD`),
//! rendered by build.rs from the embedded contract. Operation submodules hold
//! the facts of the variants that operation admits.
include!(concat!(env!("OUT_DIR"), "/query_limits.rs"));
