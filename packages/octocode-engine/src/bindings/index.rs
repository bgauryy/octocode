use std::path::{Path, PathBuf};

use napi::bindgen_prelude::AsyncTask;
use napi::{Error, Result, Status};
use napi_derive::napi;

use crate::bindings::tasks::{BuildIndexTask, IndexStatusTask, QueryIndexTask};
use crate::index::{
    IndexAccess, IndexBuildOptions, IndexQueryKind, IndexQueryOptions, IndexRuntimeLimits,
    IndexStatusOptions, RootIdentity,
};
use crate::types::{
    IndexBuildRequest, IndexBuildResult, IndexFreshnessResult, IndexQueryMatchResult,
    IndexQueryRequest, IndexQueryResult, IndexStatusRequest, IndexStatusResult, IndexStoreOptions,
};

const DEFAULT_MAX_GENERATIONS: usize = 3;
const DEFAULT_MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const DEFAULT_MAX_FILES: usize = 50_000;
const DEFAULT_MAX_ENTRIES: usize = 100_000;
const DEFAULT_MAX_DEPTH: usize = 64;
const DEFAULT_MAX_FILE_BYTES: u64 = 1024 * 1024;
const DEFAULT_MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const DEFAULT_PAGE_SIZE: usize = 100;

#[napi(js_name = "buildIndex")]
pub fn build_index(options: IndexBuildRequest) -> AsyncTask<BuildIndexTask> {
    AsyncTask::new(BuildIndexTask {
        options: Some(options),
    })
}

#[napi(js_name = "queryIndex")]
pub fn query_index(options: IndexQueryRequest) -> AsyncTask<QueryIndexTask> {
    AsyncTask::new(QueryIndexTask {
        options: Some(options),
    })
}

#[napi(js_name = "indexStatus")]
pub fn index_status(options: IndexStatusRequest) -> AsyncTask<IndexStatusTask> {
    AsyncTask::new(IndexStatusTask {
        options: Some(options),
    })
}

pub(crate) fn build_index_inner(options: IndexBuildRequest) -> Result<IndexBuildResult> {
    let access = access(options.store)?;
    let limits = IndexRuntimeLimits {
        max_files: positive_u32(options.max_files, DEFAULT_MAX_FILES, "maxFiles")?,
        max_entries: positive_u32(options.max_entries, DEFAULT_MAX_ENTRIES, "maxEntries")?,
        max_depth: options
            .max_depth
            .map_or(DEFAULT_MAX_DEPTH, |value| value as usize),
        max_file_bytes: positive_i64(
            options.max_file_bytes,
            DEFAULT_MAX_FILE_BYTES,
            "maxFileBytes",
        )?,
        max_source_bytes: positive_i64(
            options.max_source_bytes,
            DEFAULT_MAX_SOURCE_BYTES,
            "maxSourceBytes",
        )?,
    };
    let result = crate::index::build_index(IndexBuildOptions {
        access,
        exclusions: options.exclusions.unwrap_or_default(),
        limits,
    })
    .map_err(index_error)?;
    Ok(IndexBuildResult {
        generation: numeric(result.generation),
        root_id: result.root_id,
        canonical_root: display_path(&result.canonical_root),
        snapshot: result.snapshot,
        document_count: numeric(result.document_count),
        indexed_source_bytes: numeric(result.indexed_source_bytes),
        symbol_count: numeric(result.symbol_count),
        complete: result.complete,
        truncated: result.truncated,
        limit_reason: result.limit_reason,
        excluded_paths: result.excluded_paths,
        skipped_binary: result.skipped_binary,
        skipped_too_large: result.skipped_too_large,
        skipped_symlinks: result.skipped_symlinks,
        unverifiable: result.unverifiable,
        freshness: freshness(result.freshness),
        usable: result.usable,
    })
}

pub(crate) fn query_index_inner(options: IndexQueryRequest) -> Result<IndexQueryResult> {
    let access = access(options.store)?;
    let kind = match options.kind.as_str() {
        "content" => IndexQueryKind::Content,
        "symbol" => IndexQueryKind::Symbol,
        _ => {
            return Err(Error::new(
                Status::InvalidArg,
                "index query kind must be content or symbol",
            ))
        }
    };
    let expected_generation = optional_generation(options.expected_generation)?;
    let result = crate::index::query_index(IndexQueryOptions {
        access,
        text: options.text,
        kind,
        case_sensitive: options.case_sensitive.unwrap_or(true),
        offset: options.offset.unwrap_or(0) as usize,
        limit: positive_u32(options.limit, DEFAULT_PAGE_SIZE, "limit")?,
        expected_generation,
        freshness_max_entries: positive_u32(
            options.freshness_max_entries,
            DEFAULT_MAX_ENTRIES,
            "freshnessMaxEntries",
        )?,
        freshness_max_depth: options
            .freshness_max_depth
            .map_or(DEFAULT_MAX_DEPTH, |value| value as usize),
    })
    .map_err(index_error)?;
    Ok(IndexQueryResult {
        generation: numeric(result.generation),
        root_id: result.root_id,
        canonical_root: display_path(&result.canonical_root),
        snapshot: result.snapshot,
        matches: result
            .matches
            .into_iter()
            .map(|item| IndexQueryMatchResult {
                path: item.path,
                line: numeric(item.line),
                column: numeric(item.column),
                start_byte: numeric(item.start_byte),
                end_byte: numeric(item.end_byte),
                value: item.value,
                symbol_kind: item.symbol_kind,
            })
            .collect(),
        total_matches: numeric(result.total_matches),
        next_offset: result.next_offset.map(numeric),
        exclusions: result.exclusions,
        freshness: freshness(result.freshness),
        usable: result.usable,
        absence_proven: result.absence_proven,
        diagnostic: result.diagnostic,
    })
}

pub(crate) fn index_status_inner(options: IndexStatusRequest) -> Result<IndexStatusResult> {
    let access = access(options.store)?;
    let result = crate::index::index_status(IndexStatusOptions {
        access,
        freshness_max_entries: positive_u32(
            options.freshness_max_entries,
            DEFAULT_MAX_ENTRIES,
            "freshnessMaxEntries",
        )?,
        freshness_max_depth: options
            .freshness_max_depth
            .map_or(DEFAULT_MAX_DEPTH, |value| value as usize),
    })
    .map_err(index_error)?;
    Ok(IndexStatusResult {
        indexed: result.indexed,
        usable: result.usable,
        root_id: result.root_id,
        canonical_root: display_path(&result.canonical_root),
        generation: result.generation.map(numeric),
        snapshot: result.snapshot,
        document_count: numeric(result.document_count),
        indexed_source_bytes: numeric(result.indexed_source_bytes),
        exclusions: result.exclusions,
        freshness: result.freshness.map(freshness),
        diagnostic: result.diagnostic,
    })
}

fn access(options: IndexStoreOptions) -> Result<IndexAccess> {
    let root = match (options.source_commit, options.source_tree) {
        (None, None) => RootIdentity::mutable(&options.root_id, Path::new(&options.root_path)),
        (Some(commit), Some(tree)) => RootIdentity::immutable_git(
            &options.root_id,
            Path::new(&options.root_path),
            &commit,
            &tree,
        ),
        _ => {
            return Err(Error::new(
                Status::InvalidArg,
                "sourceCommit and sourceTree must be provided together",
            ))
        }
    }
    .map_err(index_error)?;
    Ok(IndexAccess {
        home: PathBuf::from(options.home),
        root,
        config: crate::index::IndexConfig {
            expected_schema_version: options.index_schema_version,
            expected_parser_schema_version: options.parser_schema_version,
            expected_tool_version: options.tool_version,
            max_generations: positive_u32(
                options.max_generations,
                DEFAULT_MAX_GENERATIONS,
                "maxGenerations",
            )?,
            max_bytes: positive_i64(options.max_bytes, DEFAULT_MAX_BYTES, "maxBytes")?,
        },
    })
}

fn freshness(report: crate::index::FreshnessReport) -> IndexFreshnessResult {
    let can_prove_absence = report.can_prove_absence();
    IndexFreshnessResult {
        checked: numeric(report.checked),
        fresh: numeric(report.fresh),
        generation_complete: report.generation_complete,
        traversal_complete: report.traversal_complete,
        scanned_entries: numeric(report.scanned_entries),
        added: report.added,
        dirty: report.dirty,
        deleted: report.deleted,
        unverifiable: report.unverifiable,
        can_prove_absence,
    }
}

fn positive_u32(value: Option<u32>, default: usize, name: &str) -> Result<usize> {
    match value {
        Some(0) => Err(Error::new(
            Status::InvalidArg,
            format!("{name} must be positive"),
        )),
        Some(value) => Ok(value as usize),
        None => Ok(default),
    }
}

fn positive_i64(value: Option<i64>, default: u64, name: &str) -> Result<u64> {
    match value {
        Some(value) if value <= 0 => Err(Error::new(
            Status::InvalidArg,
            format!("{name} must be positive"),
        )),
        Some(value) => Ok(value as u64),
        None => Ok(default),
    }
}

fn optional_generation(value: Option<i64>) -> Result<Option<u64>> {
    match value {
        Some(value) if value <= 0 => Err(Error::new(
            Status::InvalidArg,
            "expectedGeneration must be positive",
        )),
        Some(value) => Ok(Some(value as u64)),
        None => Ok(None),
    }
}

fn index_error(error: crate::index::IndexError) -> Error {
    Error::new(Status::GenericFailure, error.to_string())
}

fn numeric(value: impl TryInto<i64>) -> i64 {
    value.try_into().unwrap_or(i64::MAX)
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
