//! astSearch `match`: file and match pages, clipped values, captures,
//! scan windows and the reads a match offers.
use super::test_rows::*;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::test_support::Fixture;
use serde_json::json;

mod absence;
mod grammars;
mod key_blocks;
mod pages;
mod rows;
mod source_limit;
mod stream;
mod values;
