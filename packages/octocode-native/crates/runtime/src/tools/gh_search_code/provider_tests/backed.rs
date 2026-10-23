//! ghSearchCode against the mock server: hit rows, ref reads, partial and empty pages.
use crate::tools::gh_shared::test_support::backed::*;

mod empty;
mod hits;
mod pages;
mod refs;
mod repository;
