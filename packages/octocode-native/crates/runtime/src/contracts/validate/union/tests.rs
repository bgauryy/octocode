//! Union branch selection tests: the fast acceptance path against the
//! reference algorithm.

use crate::contracts::prepare_many_and_validate;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
mod equivalence;
mod messages;
