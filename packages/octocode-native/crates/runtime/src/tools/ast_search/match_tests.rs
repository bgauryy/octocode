//! astSearch `match`: file and match pages, clipped values, captures,
//! scan windows and the reads a match offers.
use super::test_rows::*;
use crate::tools::test_support::Fixture;
use octocode_contract_types::ErrorCode;
use serde_json::json;

mod grammars;
mod pages;
mod rows;
mod values;
