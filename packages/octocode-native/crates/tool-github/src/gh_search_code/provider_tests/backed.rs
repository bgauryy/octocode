//! ghSearchCode against the mock server: hit rows, ref reads, partial and empty pages.
use crate::test_support::backed::run_code as run;
use crate::test_support::backed::*;

mod empty;
mod hits;
mod pages;
mod refs;
mod repository;
