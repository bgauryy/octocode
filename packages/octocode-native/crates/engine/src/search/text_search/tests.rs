use super::collect::*;
use super::file_search::*;
use super::mode::*;
use super::pcre2::*;
use super::result::*;
use super::sink::*;
use super::state::*;
use super::*;
use crate::error::Result;
use crate::search::text_parser::FileEntry;
use crate::test_support::TmpDir;
use crate::text::source::open_regular;
use crate::types::{
    CapReason, CaseRule, PatternSyntax, SearchOutput, SearchSort, SourceValues, SpanValues,
    TextSearchFile, TextSearchOptions, TextSearchResult, TextSearchStats,
};
use ignore::types::TypesBuilder;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

mod coverage;
mod digests;
mod limits;
mod matching;
mod ordering;
mod ranking;
mod selection;
mod snippets;
mod spans;
mod walk_parity;

struct AllowAll;

impl TextSearchPathFilter for AllowAll {
    fn allows(&self, _: &Path, _: bool) -> bool {
        true
    }
}

pub(crate) fn search(opts: TextSearchOptions) -> Result<TextSearchResult> {
    search_cancellable(opts, Arc::new(AllowAll), &|| false)
}

fn opts(path: String, pattern: &str) -> TextSearchOptions {
    TextSearchOptions {
        path,
        pattern: pattern.to_owned(),
        ..Default::default()
    }
}
