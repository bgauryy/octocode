//! Union branch selection tests: the fast acceptance path against the
//! reference algorithm.

use crate::contracts::prepare_rows;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
mod absorption;
mod allowed_fields;
mod branch_truth;
mod enums;
mod equivalence;
mod messages;
