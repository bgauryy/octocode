//! astSearch patterns and rules: compile errors, exact patterns, rule
//! spellings, and syntax-tree error rows.
use super::test_rows::*;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::test_support::{Fixture, checks, simple_policy};
use serde_json::json;
use std::path::PathBuf;

mod patterns;
mod qualified;
mod rules;
mod syntax_tree;
