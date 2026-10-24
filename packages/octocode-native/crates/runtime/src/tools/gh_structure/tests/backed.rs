//! ghStructure against the mock server: listings, pinned pages, materialize.
use crate::tools::gh_shared::test_support::backed::run_tree as run;
use crate::tools::gh_shared::test_support::backed::*;
mod errors;
mod leads;
mod paging;
mod walk;
