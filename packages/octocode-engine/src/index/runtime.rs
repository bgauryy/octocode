use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use super::store::{read_source_bytes, CapturedSource};
use super::{
    query_documents, FreshnessReport, GenerationSpec, IndexConfig, IndexError, IndexQuery,
    IndexQueryKind, IndexQueryMatch, IndexStore, Result, RootIdentity, SymbolRecord,
};

#[derive(Clone, Debug)]
pub struct IndexAccess {
    pub home: PathBuf,
    pub root: RootIdentity,
    pub config: IndexConfig,
}

#[derive(Clone, Debug)]
pub struct IndexRuntimeLimits {
    pub max_files: usize,
    pub max_entries: usize,
    pub max_depth: usize,
    pub max_file_bytes: u64,
    pub max_source_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct IndexBuildOptions {
    pub access: IndexAccess,
    pub exclusions: Vec<String>,
    pub limits: IndexRuntimeLimits,
}

#[derive(Clone, Debug)]
pub struct IndexBuildResult {
    pub generation: u64,
    pub root_id: String,
    pub canonical_root: PathBuf,
    pub snapshot: String,
    pub document_count: u64,
    pub indexed_source_bytes: u64,
    pub symbol_count: usize,
    pub complete: bool,
    pub truncated: bool,
    pub limit_reason: Option<String>,
    pub excluded_paths: Vec<String>,
    pub skipped_binary: Vec<String>,
    pub skipped_too_large: Vec<String>,
    pub skipped_symlinks: Vec<String>,
    pub unverifiable: Vec<String>,
    pub freshness: FreshnessReport,
    pub usable: bool,
}

#[derive(Clone, Debug)]
pub struct IndexQueryOptions {
    pub access: IndexAccess,
    pub text: String,
    pub kind: IndexQueryKind,
    pub case_sensitive: bool,
    pub offset: usize,
    pub limit: usize,
    pub expected_generation: Option<u64>,
    pub freshness_max_entries: usize,
    pub freshness_max_depth: usize,
}

#[derive(Clone, Debug)]
pub struct IndexQueryRuntimeResult {
    pub generation: u64,
    pub root_id: String,
    pub canonical_root: PathBuf,
    pub snapshot: String,
    pub matches: Vec<IndexQueryMatch>,
    pub total_matches: usize,
    pub next_offset: Option<usize>,
    pub exclusions: Vec<String>,
    pub freshness: FreshnessReport,
    pub usable: bool,
    pub absence_proven: bool,
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug)]
pub struct IndexStatusOptions {
    pub access: IndexAccess,
    pub freshness_max_entries: usize,
    pub freshness_max_depth: usize,
}

#[derive(Clone, Debug)]
pub struct IndexStatus {
    pub indexed: bool,
    pub usable: bool,
    pub root_id: String,
    pub canonical_root: PathBuf,
    pub generation: Option<u64>,
    pub snapshot: Option<String>,
    pub document_count: u64,
    pub indexed_source_bytes: u64,
    pub exclusions: Vec<String>,
    pub freshness: Option<FreshnessReport>,
    pub diagnostic: Option<String>,
}

struct Traversal {
    files: Vec<String>,
    skipped_symlinks: Vec<String>,
    unverifiable: Vec<String>,
    complete: bool,
    limit_reason: Option<String>,
}

pub fn build_index(options: IndexBuildOptions) -> Result<IndexBuildResult> {
    build_index_with_snapshot_observer(options, || {})
}

pub(super) fn build_index_with_snapshot_observer(
    options: IndexBuildOptions,
    after_capture: impl FnOnce(),
) -> Result<IndexBuildResult> {
    validate_build_limits(&options.limits)?;
    let root = options.access.root.canonical_root.clone();
    let mut traversal = traverse_files(
        &root,
        &options.exclusions,
        options.limits.max_entries,
        options.limits.max_depth,
    );
    traversal.files.sort();
    if traversal.files.len() > options.limits.max_files {
        traversal.files.truncate(options.limits.max_files);
        traversal.complete = false;
        traversal.limit_reason = Some("maxFiles".to_owned());
    }

    let store = IndexStore::open(
        options.access.home.clone(),
        &options.access.root.root_id,
        options.access.config.clone(),
    )?;
    let mut effective_exclusions = normalized_exclusions(options.exclusions)?;
    let mut skipped_binary = Vec::new();
    let mut skipped_too_large = Vec::new();
    let mut source_bytes = 0_u64;
    let mut candidates = Vec::new();

    for relative in &traversal.files {
        let path = root.join(relative);
        let metadata = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            _ => {
                traversal.unverifiable.push(relative.clone());
                traversal.complete = false;
                continue;
            }
        };
        if metadata.len() > options.limits.max_file_bytes {
            skipped_too_large.push(relative.clone());
            effective_exclusions.push(relative.clone());
            continue;
        }
        if source_bytes.saturating_add(metadata.len()) > options.limits.max_source_bytes {
            traversal.complete = false;
            traversal.limit_reason = Some("maxSourceBytes".to_owned());
            break;
        }
        let remaining_bytes = options.limits.max_source_bytes.saturating_sub(source_bytes);
        let read_limit = options.limits.max_file_bytes.min(remaining_bytes);
        let (bytes, metadata) = match read_source_bytes(&root, Path::new(relative), read_limit) {
            Ok(source) => source,
            Err(_) => {
                traversal.unverifiable.push(relative.clone());
                traversal.complete = false;
                continue;
            }
        };
        if bytes.len() as u64 > options.limits.max_file_bytes {
            skipped_too_large.push(relative.clone());
            effective_exclusions.push(relative.clone());
            continue;
        }
        if bytes.len() as u64 > remaining_bytes {
            traversal.complete = false;
            traversal.limit_reason = Some("maxSourceBytes".to_owned());
            break;
        }
        let content = match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(_) => {
                skipped_binary.push(relative.clone());
                effective_exclusions.push(relative.clone());
                continue;
            }
        };
        source_bytes = source_bytes.saturating_add(content.len() as u64);
        let language = language_for_path(Path::new(relative));
        let symbols = extract_symbols(&content, relative);
        let source = CapturedSource::new(content, &metadata);
        candidates.push((relative.clone(), language, source, symbols));
    }

    after_capture();
    effective_exclusions.extend(traversal.skipped_symlinks.iter().cloned());
    effective_exclusions.sort();
    effective_exclusions.dedup();
    traversal.unverifiable.sort();
    traversal.unverifiable.dedup();

    let complete = traversal.complete && traversal.unverifiable.is_empty();
    let mut writer = store.begin_generation(GenerationSpec {
        root: options.access.root.clone(),
        index_schema_version: options.access.config.expected_schema_version,
        parser_schema_version: options.access.config.expected_parser_schema_version,
        tool_version: options.access.config.expected_tool_version.clone(),
        exclusions: effective_exclusions.clone(),
        complete,
    })?;
    let mut symbol_count = 0_usize;
    for (relative, language, source, symbols) in candidates {
        symbol_count = symbol_count.saturating_add(symbols.len());
        writer.add_captured_file(&root, relative, &language, source, symbols)?;
    }
    let manifest = writer.commit()?;
    let reader = store.open_active(&options.access.root)?;
    let freshness =
        reader.verify_strict_bounded(&root, options.limits.max_entries, options.limits.max_depth);
    let usable = freshness.can_prove_absence();
    Ok(IndexBuildResult {
        generation: manifest.generation,
        root_id: manifest.root.root_id.clone(),
        canonical_root: manifest.root.canonical_root.clone(),
        snapshot: manifest.manifest_digest.clone(),
        document_count: manifest.document_count,
        indexed_source_bytes: manifest.indexed_source_bytes,
        symbol_count,
        complete: manifest.complete,
        truncated: traversal.limit_reason.is_some(),
        limit_reason: traversal.limit_reason,
        excluded_paths: effective_exclusions,
        skipped_binary,
        skipped_too_large,
        skipped_symlinks: traversal.skipped_symlinks,
        unverifiable: traversal.unverifiable,
        freshness,
        usable,
    })
}

pub fn query_index(options: IndexQueryOptions) -> Result<IndexQueryRuntimeResult> {
    if options.text.is_empty() {
        return Err(IndexError::InvalidConfig(
            "index query text must be non-empty".to_owned(),
        ));
    }
    if options.limit == 0 || options.limit > 1_000 {
        return Err(IndexError::InvalidConfig(
            "index query limit must be between 1 and 1000".to_owned(),
        ));
    }
    validate_freshness_limits(options.freshness_max_entries, options.freshness_max_depth)?;
    let store = IndexStore::open(
        options.access.home,
        &options.access.root.root_id,
        options.access.config,
    )?;
    let reader = store.open_active(&options.access.root)?;
    if let Some(expected) = options.expected_generation {
        if expected != reader.generation() {
            return Err(IndexError::GenerationMismatch {
                expected,
                actual: reader.generation(),
            });
        }
    }
    let freshness = reader.verify_strict_bounded(
        &options.access.root.canonical_root,
        options.freshness_max_entries,
        options.freshness_max_depth,
    );
    let usable = freshness.can_prove_absence();
    let generation = reader.generation();
    let root_id = reader.manifest().root.root_id.clone();
    let canonical_root = reader.manifest().root.canonical_root.clone();
    let snapshot = reader.manifest().manifest_digest.clone();
    let exclusions = reader.manifest().exclusions.clone();
    if !usable {
        return Ok(IndexQueryRuntimeResult {
            generation,
            root_id,
            canonical_root,
            snapshot,
            matches: Vec::new(),
            total_matches: 0,
            next_offset: None,
            exclusions,
            freshness,
            usable: false,
            absence_proven: false,
            diagnostic: Some("index.stale".to_owned()),
        });
    }
    let result = query_documents(
        reader.documents(),
        &IndexQuery {
            text: options.text,
            kind: options.kind,
            case_sensitive: options.case_sensitive,
            offset: options.offset,
            limit: options.limit,
        },
    );
    Ok(IndexQueryRuntimeResult {
        generation,
        root_id,
        canonical_root,
        snapshot,
        absence_proven: result.total_matches == 0,
        matches: result.matches,
        total_matches: result.total_matches,
        next_offset: result.next_offset,
        exclusions,
        freshness,
        usable: true,
        diagnostic: None,
    })
}

pub fn index_status(options: IndexStatusOptions) -> Result<IndexStatus> {
    validate_freshness_limits(options.freshness_max_entries, options.freshness_max_depth)?;
    let store = IndexStore::open(
        options.access.home,
        &options.access.root.root_id,
        options.access.config,
    )?;
    let reader = match store.open_active(&options.access.root) {
        Ok(reader) => reader,
        Err(IndexError::NoActiveGeneration) => {
            return Ok(IndexStatus {
                indexed: false,
                usable: false,
                root_id: options.access.root.root_id,
                canonical_root: options.access.root.canonical_root,
                generation: None,
                snapshot: None,
                document_count: 0,
                indexed_source_bytes: 0,
                exclusions: Vec::new(),
                freshness: None,
                diagnostic: Some("index.absent".to_owned()),
            })
        }
        Err(error) => return Err(error),
    };
    let freshness = reader.verify_strict_bounded(
        &options.access.root.canonical_root,
        options.freshness_max_entries,
        options.freshness_max_depth,
    );
    let usable = freshness.can_prove_absence();
    Ok(IndexStatus {
        indexed: true,
        usable,
        root_id: reader.manifest().root.root_id.clone(),
        canonical_root: reader.manifest().root.canonical_root.clone(),
        generation: Some(reader.generation()),
        snapshot: Some(reader.manifest().manifest_digest.clone()),
        document_count: reader.manifest().document_count,
        indexed_source_bytes: reader.manifest().indexed_source_bytes,
        exclusions: reader.manifest().exclusions.clone(),
        freshness: Some(freshness),
        diagnostic: (!usable).then(|| "index.stale".to_owned()),
    })
}

fn validate_build_limits(limits: &IndexRuntimeLimits) -> Result<()> {
    if limits.max_files == 0 || limits.max_files > 100_000 {
        return Err(IndexError::InvalidConfig(
            "max_files must be between 1 and 100000".to_owned(),
        ));
    }
    if limits.max_file_bytes == 0 || limits.max_file_bytes > 16 * 1024 * 1024 {
        return Err(IndexError::InvalidConfig(
            "max_file_bytes must be between 1 and 16777216".to_owned(),
        ));
    }
    if limits.max_source_bytes == 0 || limits.max_source_bytes > 64 * 1024 * 1024 * 1024 {
        return Err(IndexError::InvalidConfig(
            "max_source_bytes must be between 1 and 68719476736".to_owned(),
        ));
    }
    validate_freshness_limits(limits.max_entries, limits.max_depth)
}

fn validate_freshness_limits(max_entries: usize, max_depth: usize) -> Result<()> {
    if max_entries == 0 || max_entries > 1_000_000 {
        return Err(IndexError::InvalidConfig(
            "max_entries must be between 1 and 1000000".to_owned(),
        ));
    }
    if max_depth > 256 {
        return Err(IndexError::InvalidConfig(
            "max_depth must not exceed 256".to_owned(),
        ));
    }
    Ok(())
}

fn normalized_exclusions(exclusions: Vec<String>) -> Result<Vec<String>> {
    let mut normalized = Vec::new();
    for exclusion in exclusions {
        let value = exclusion.trim_matches('/').replace('\\', "/");
        let path = Path::new(&value);
        if value.is_empty()
            || path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(IndexError::InvalidConfig(format!(
                "invalid index exclusion: {exclusion}"
            )));
        }
        normalized.push(value);
    }
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn traverse_files(
    root: &Path,
    exclusions: &[String],
    max_entries: usize,
    max_depth: usize,
) -> Traversal {
    let mut traversal = Traversal {
        files: Vec::new(),
        skipped_symlinks: Vec::new(),
        unverifiable: Vec::new(),
        complete: true,
        limit_reason: None,
    };
    let mut pending = VecDeque::from([(root.to_path_buf(), 0_usize)]);
    let mut scanned = 0_usize;
    while let Some((directory, depth)) = pending.pop_front() {
        let remaining = max_entries.saturating_sub(scanned);
        if remaining == 0 {
            traversal.complete = false;
            traversal.limit_reason = Some("maxEntries".to_owned());
            break;
        }
        let entries = match read_directory_batch(&directory, remaining) {
            Ok(Some(entries)) => entries,
            Ok(None) => {
                traversal.complete = false;
                traversal.limit_reason = Some("maxEntries".to_owned());
                break;
            }
            Err(_) => {
                traversal.unverifiable.push(relative_path(root, &directory));
                traversal.complete = false;
                continue;
            }
        };
        scanned = scanned.saturating_add(entries.len());
        for path in entries {
            let relative = relative_path(root, &path);
            if path_is_excluded(&relative, exclusions) {
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    traversal.unverifiable.push(relative);
                    traversal.complete = false;
                    continue;
                }
            };
            if metadata.file_type().is_symlink() {
                traversal.skipped_symlinks.push(relative);
            } else if metadata.is_dir() {
                if depth >= max_depth {
                    traversal.unverifiable.push(relative);
                    traversal.complete = false;
                } else {
                    pending.push_back((path, depth + 1));
                }
            } else if metadata.is_file() {
                traversal.files.push(relative);
            } else {
                traversal.unverifiable.push(relative);
                traversal.complete = false;
            }
        }
    }
    traversal.files.sort();
    traversal.skipped_symlinks.sort();
    traversal.unverifiable.sort();
    traversal
}

fn read_directory_batch(
    directory: &Path,
    remaining: usize,
) -> std::io::Result<Option<Vec<PathBuf>>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)? {
        entries.push(entry?.path());
        if entries.len() > remaining {
            return Ok(None);
        }
    }
    entries.sort();
    Ok(Some(entries))
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn path_is_excluded(relative: &str, exclusions: &[String]) -> bool {
    exclusions.iter().any(|exclusion| {
        let normalized = exclusion.trim_matches('/');
        relative == normalized
            || relative.starts_with(&format!("{normalized}/"))
            || (!normalized.contains('/') && relative.split('/').any(|part| part == normalized))
    })
}

fn language_for_path(path: &Path) -> String {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    crate::signatures::languages::find_entry(&extension)
        .map_or_else(|| extension.clone(), |entry| entry.name.to_owned())
}

fn extract_symbols(content: &str, file_path: &str) -> Vec<SymbolRecord> {
    let Some(extraction) =
        crate::signatures::extract_graph_facts_with_metadata_inner(content, file_path)
    else {
        return Vec::new();
    };
    extraction
        .facts
        .declarations
        .into_iter()
        .filter_map(|declaration| {
            let start = position_to_byte(content, &declaration.selection_range.start)?;
            let end = position_to_byte(content, &declaration.selection_range.end)?.max(start);
            Some(SymbolRecord {
                name: declaration.name,
                kind: declaration.kind,
                start_byte: start as u64,
                end_byte: end as u64,
            })
        })
        .collect()
}

fn position_to_byte(content: &str, position: &crate::graph::GraphPosition) -> Option<usize> {
    let line = usize::try_from(position.line).ok()?;
    let character = usize::try_from(position.character).ok()?;
    let line_text = content.split_inclusive('\n').nth(line)?;
    let line_start = content
        .split_inclusive('\n')
        .take(line)
        .map(str::len)
        .sum::<usize>();
    Some(line_start + crate::text::utf8_offsets::char_to_byte_offset_inner(line_text, character))
}
