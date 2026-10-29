//! astSearch patterns and rules: compile errors, exact patterns, rule
//! spellings, and syntax-tree error rows.
use super::test_rows::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, checks, simple_policy};
use octocode_contract_types::ErrorCode;
use serde_json::json;
use std::path::PathBuf;

mod patterns;
mod qualified;
mod rules;
mod syntax_tree;
