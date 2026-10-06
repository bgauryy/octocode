use crate::digest::{json_sha256, sha256};
use crate::tools::ast_rule;
use crate::tools::id::ToolId;
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::cancel::CancellationCheck,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

mod journal;
mod lock;
mod output;
pub(crate) use output::Output;

use journal::{commit_transaction, recover_transactions};
use lock::RootLock;
use output::{attach_receipts, continuation_query, portable_relative, success_value};
mod patch;
mod raw;
mod request;
mod staged;
use patch::create_unified_patch;
use raw::{RawByteRange, RawCapture, RawMatch, RawMetaVariables, RawPosition, RawRange};
use staged::{StagedAnalyzer, StagedFacts, note_parses};

const DEFAULT_MAX_PATCH_BYTES: usize = 512 * 1024;
const MAX_FILE_BYTES: usize = 1_000_000;
const JOURNAL_PREFIX: &str = ".octocode-ast-rewrite-journal-";
static APPLY_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug)]
pub struct AstRewriteRuntimeOptions {
    pub allow_apply: bool,
    pub max_patch_bytes: usize,
}

impl Default for AstRewriteRuntimeOptions {
    fn default() -> Self {
        Self {
            allow_apply: false,
            max_patch_bytes: DEFAULT_MAX_PATCH_BYTES,
        }
    }
}

#[derive(Clone, Debug)]
struct ExecutableReceipt {
    path: PathBuf,
    version: String,
    sha256: String,
    capability_digest: String,
    capabilities: Vec<String>,
}

struct PrepareContext<'a> {
    boundary: &'a Path,
    paths: &'a PathPolicy,
    security: &'a ContentSecurity,
    cancellation: &'a dyn CancellationCheck,
    options: &'a AstRewriteRuntimeOptions,
    analyzer: &'a StagedAnalyzer,
}

pub use request::{ArPostconditionsItem, AstRewriteQuery, RewriteRequest};

#[derive(Clone, Debug)]
struct PreparedMatch {
    public: Value,
    id: String,
    start: usize,
    end: usize,
    expected: Vec<u8>,
    replacement: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PreparedFile {
    path: String,
    absolute: PathBuf,
    before_hash: String,
    after_hash: String,
    before: Vec<u8>,
    after: Vec<u8>,
    patch: String,
    matches: Vec<PreparedMatch>,
    permissions: fs::Permissions,
    /// ERROR/MISSING nodes in `before`, from the scan's parse.
    before_errors: u32,
    /// Facts from the single parse of `after` (`None`: over the parse bound).
    after_facts: Option<StagedFacts>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Journal {
    version: u8,
    id: String,
    root: PathBuf,
    phase: JournalPhase,
    files: Vec<JournalFile>,
}

/// How far a transaction got: recovery finalizes a committed one and rolls
/// back any other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum JournalPhase {
    Staging,
    Prepared,
    Committing,
    Committed,
}

/// How far one file of a transaction got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum FileState {
    Planned,
    Staged,
    BackedUp,
    Promoted,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct JournalFile {
    target: PathBuf,
    stage: PathBuf,
    backup: PathBuf,
    before_hash: String,
    after_hash: String,
    state: FileState,
}

#[derive(Debug)]
struct RewriteError {
    code: &'static str,
    message: String,
    details: Option<Box<Value>>,
    terminal: bool,
    next: Option<Box<Value>>,
}

impl RewriteError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
            terminal: false,
            next: None,
        }
    }
    fn detail(mut self, value: Value) -> Self {
        self.details = Some(Box::new(value));
        self
    }
    fn terminal(mut self) -> Self {
        self.terminal = true;
        self
    }
    fn restart(mut self, query: &RewriteRequest) -> Self {
        let mut restart = continuation_query(query, Path::new(&query.path()));
        for key in [
            "snapshot",
            "expectedHashes",
            "selectedMatchIds",
            "postconditions",
        ] {
            restart.as_object_mut().map(|object| object.remove(key));
        }
        restart["apply"] = json!(false);
        restart["page"] = json!(1);
        self.next = Some(Box::new(json!({
            "restart": crate::tools::result::Continuation::new(ToolId::AstRewrite, restart)
                .confidence("exact")
                .build()
        })));
        self
    }
    fn value(self) -> Value {
        // `status:"error"` marks the row for the engine's result shaping;
        // without it `result_row` strips the `error` message from the output.
        let mut value = json!({
            "operation":"rewrite",
            "status":"error",
            "errorCode":self.code,
            "error":self.message
        });
        if let Some(hint) = recovery_hint(self.code) {
            value["hints"] = json!([hint]);
        }
        if self.terminal {
            value["isPartial"] = json!(true);
            value["terminalLimit"] = json!(true);
        }
        if let Some(details) = self.details {
            value["details"] = *details;
        }
        if let Some(next) = self.next {
            value["next"] = *next;
        }
        value
    }
}

/// Code-specific recovery for the preview→apply guard failures, so each error
/// names its own fix instead of a generic "preview again" fallback.
fn recovery_hint(code: &str) -> Option<&'static str> {
    Some(match code {
        "ast.rewrite.snapshot_required" | "ast.rewrite.expected_hashes_required" => {
            "Run the complete preview's hints.apply unchanged; it carries snapshot and expectedHashes."
        }
        "ast.rewrite.expected_hash_invalid" => {
            "Copy expectedHashes from hints.apply (paged previews: files[].path → beforeHash), or run hints.apply unchanged."
        }
        "ast.rewrite.expected_hash_set_mismatch" | "ast.rewrite.expected_hash_missing" => {
            "expectedHashes must list exactly the preview's affected files; run the complete preview's hints.apply unchanged."
        }
        "ast.rewrite.hash_mismatch" => {
            "A file changed since preview; follow next.restart, then apply with the new preview's hints.apply."
        }
        "ast.rewrite.source_mismatch" => {
            "Match bytes or the generated replacement disagree with the verified source; preview again with the same query."
        }
        "staleSnapshot" => {
            "Discard earlier preview pages; follow next.restart and page the new preview to its hints.apply."
        }
        "ast.rewrite.broken_syntax" => {
            "Fix the rewrite template so the replaced node still parses (keep its delimiters and $$$ lists); details.path names the file."
        }
        ast_rule::INVALID_PATTERN => ast_rule::INVALID_PATTERN_HINT,
        "ast.rewrite.overlap" => {
            "Innermost first: rule {pattern:P, not:{has:{pattern:P with new $VARS, stopBy:\"end\"}}}; apply, repeat. Or narrow path."
        }
        "ast.rewrite.postcondition_failed" => {
            "remainingMatches counts only the files this apply rewrites; adjust equals or the selection, then preview again."
        }
        _ => return None,
    })
}

/// Execute one canonical structural-rewrite query. Domain failures are returned
/// as typed result values so bulk callers retain per-query diagnostics.
pub fn execute_ast_rewrite_with_options(
    query: RewriteRequest,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancellation: &dyn CancellationCheck,
    options: &AstRewriteRuntimeOptions,
) -> Value {
    match execute(query, paths, security, cancellation, options) {
        Ok(value) => value,
        Err(error) => error.value(),
    }
}

fn execute(
    mut query: RewriteRequest,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancellation: &dyn CancellationCheck,
    options: &AstRewriteRuntimeOptions,
) -> Result<Value, RewriteError> {
    cancellation.check().map_err(cancelled)?;
    admit(&query, options)?;
    let (root, boundary) = open_root(&mut query, paths, cancellation)?;
    // Only apply mutates: a preview reads without the locks or journal
    // recovery (the snapshot and expectedHashes guard the apply that follows).
    // A poisoned process lock (a prior panic while holding it) is recovered
    // instead of bricking astRewrite for the rest of the process lifetime.
    let _process_guard = query
        .apply()
        .then(|| APPLY_LOCK.lock().unwrap_or_else(|error| error.into_inner()));
    let lock = if query.apply() {
        let lock = RootLock::acquire(&boundary)?;
        recover_transactions(&boundary, cancellation)?;
        Some(lock)
    } else {
        None
    };
    let executable = embedded_engine_receipt();
    // Compiling validates the rule; no probe parse.
    let analyzer = StagedAnalyzer::new(&query)?;
    let (prepared, coverage) = prepare(
        &query,
        &root,
        &PrepareContext {
            boundary: &boundary,
            paths,
            security,
            cancellation,
            options,
            analyzer: &analyzer,
        },
    )?;
    let snapshot = snapshot(&query, &root, &prepared, &executable);
    if (query.apply() || query.page() > 1) && query.snapshot() != Some(snapshot.as_str()) {
        return Err(RewriteError::new(
            "staleSnapshot",
            "The source, executable, query, or selected file set changed. Preview again before continuing.",
        )
        .detail(json!({"snapshot":snapshot}))
        .restart(&query));
    }
    let mut value = if prepared.is_empty() {
        let mut empty = json!({
            "status":"empty","operation":"rewrite",
            "mode":if query.apply() {"apply"} else {"preview"},
            "root":root,
            "totalMatches":0,"affectedFiles":0,"matches":[],"files":[]
        });
        attach_receipts(&mut empty, &query, &executable);
        empty
    } else {
        let all_matches = prepared
            .iter()
            .flat_map(|file| file.matches.iter().cloned())
            .collect::<Vec<_>>();
        let (files, matches, transaction) = if query.apply() {
            let (files, matches) = select(
                &query,
                &prepared,
                &all_matches,
                options.max_patch_bytes,
                &analyzer,
            )?;
            let transaction = commit(&query, &files, &boundary, paths, cancellation)?;
            (files, matches, Some(transaction))
        } else {
            (prepared, all_matches, None)
        };
        success_value(
            &query,
            &root,
            &snapshot,
            &files,
            &matches,
            &executable,
            transaction,
        )
    };
    drop(lock);
    mark_gaps(&mut value, &coverage);
    Ok(value)
}

/// Rejects what no scan can fix: blank fields, an apply without the
/// capability or without its preview snapshot.
fn admit(query: &RewriteRequest, options: &AstRewriteRuntimeOptions) -> Result<(), RewriteError> {
    validate_query(query)?;
    if query.apply() && !options.allow_apply {
        return Err(RewriteError::new(
            "ast.rewrite.apply_disabled",
            "Applying rewrites requires the separate astRewrite apply capability.",
        ));
    }
    if query.apply() && query.snapshot().is_none() {
        return Err(RewriteError::new(
            "ast.rewrite.snapshot_required",
            "Apply requires the exact snapshot returned by preview.",
        ));
    }
    Ok(())
}

/// The canonical root and the directory that bounds every write; infers
/// the parser when `language` is omitted.
fn open_root(
    query: &mut RewriteRequest,
    paths: &PathPolicy,
    cancellation: &dyn CancellationCheck,
) -> Result<(PathBuf, PathBuf), RewriteError> {
    let validated = paths.validate(query.path()).map_err(|error| {
        RewriteError::new(
            error.local_error_code("ast.rewrite.root_unavailable"),
            error.message,
        )
    })?;
    let root = validated.canonical;
    let metadata = fs::metadata(&root).map_err(io_error)?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(RewriteError::new(
            "ast.rewrite.root_invalid",
            "The requested rewrite path must be a file or directory.",
        ));
    }
    if query.language().is_none() {
        query.inferred_lang = Some(infer_language(
            query,
            &root,
            metadata.is_dir(),
            paths,
            cancellation,
        )?);
    }
    let boundary = if metadata.is_dir() {
        root.clone()
    } else {
        root.parent().unwrap_or(&root).to_path_buf()
    };
    Ok((root, boundary))
}

/// Checks the preview's hashes and postconditions, then commits the
/// selected files in one transaction.
fn commit(
    query: &RewriteRequest,
    files: &[PreparedFile],
    boundary: &Path,
    paths: &PathPolicy,
    cancellation: &dyn CancellationCheck,
) -> Result<Value, RewriteError> {
    validate_expected_hashes(query, files, boundary, paths).map_err(|error| {
        if error.code == "ast.rewrite.hash_mismatch" {
            error.restart(query)
        } else {
            error
        }
    })?;
    validate_postconditions(query, files, cancellation)?;
    commit_transaction(boundary, files, cancellation)
}

/// A scan that skipped files is partial at its limit, and says why.
fn mark_gaps(value: &mut Value, coverage: &RewriteCoverage) {
    if !coverage.has_gaps() {
        if value["status"] == "empty" {
            value["isPartial"] = json!(false);
        }
        return;
    }
    value["isPartial"] = json!(true);
    value["coverage"] = coverage.to_json();
    if value["status"] != "empty" {
        value["terminalLimit"] = json!(true);
    }
    match value.get_mut("warnings").and_then(Value::as_array_mut) {
        Some(warnings) => warnings.push(json!(coverage.warning())),
        None => value["warnings"] = json!([coverage.warning()]),
    }
}

fn validate_query(query: &RewriteRequest) -> Result<(), RewriteError> {
    if query.path().trim().is_empty()
        || query
            .language()
            .is_some_and(|language| language.trim().is_empty())
    {
        return Err(RewriteError::new(
            "ast.rewrite.input.invalid",
            "path and language must not be blank.",
        ));
    }
    // The wire type requires each rule kind's fields; only a blank pattern
    // gets past it.
    if query
        .pattern()
        .is_some_and(|pattern| pattern.trim().is_empty())
    {
        return Err(RewriteError::new(
            "ast.rewrite.input.invalid",
            "pattern must not be blank.",
        ));
    }
    if query.apply() {
        let hashes = query.expected_hashes().ok_or_else(|| {
            RewriteError::new(
                "ast.rewrite.expected_hashes_required",
                "Apply requires expectedHashes copied from preview.",
            )
        })?;
        if hashes.is_empty() {
            return Err(RewriteError::new(
                "ast.rewrite.expected_hashes_required",
                "Apply requires non-empty expectedHashes copied from preview.",
            ));
        }
    }
    Ok(())
}

/// The parser for a query without `language`: a file's extension, or the one
/// grammar that both occurs under a directory and compiles the rule.
fn infer_language(
    query: &RewriteRequest,
    root: &Path,
    directory: bool,
    paths: &PathPolicy,
    cancellation: &dyn CancellationCheck,
) -> Result<String, RewriteError> {
    let required = |message: String| RewriteError::new("ast.rewrite.language_required", message);
    let candidates = if directory {
        let prune =
            crate::policy::prune::PruneMode::SyntaxVisible.directories(query.default_excludes());
        ast_rule::present_grammars(root, &prune, false, false, query.max_files(), &|path| {
            cancellation.check()?;
            Ok(paths.permits_discovery(path))
        })
        .map_err(required)?
        .into_iter()
        .collect::<Vec<_>>()
    } else {
        ast_rule::file_grammar(root).into_iter().collect()
    };
    let choice = ast_rule::choose_grammar(candidates.into_iter(), |language, extensions| {
        let mut probe = query.clone();
        probe.inferred_lang = Some(language.to_owned());
        octocode_engine::structural::compile_rewrite(rule_config(&probe))
            .map(drop)
            .map_err(|error| rule_error(query, extensions, &error))
    });
    match choice {
        ast_rule::GrammarChoice::One(language) => Ok(language),
        ast_rule::GrammarChoice::Invalid(error) => {
            Err(RewriteError::new(error.code, error.message))
        }
        ast_rule::GrammarChoice::Absent => Err(required(
            "No supported grammar under path compiles this rule; set language.".to_owned(),
        )),
        ast_rule::GrammarChoice::Several(several) => Err(required(format!(
            "Several grammars under path compile this rule ({}); set language to one of them.",
            several.join(", ")
        ))),
    }
}

/// [`rule_error`] for the request's own parser.
fn compile_error(query: &RewriteRequest, error: String) -> RewriteError {
    let extensions = ast_rule::language_extensions(query.lang()).unwrap_or_default();
    let error = rule_error(query, &extensions, &error);
    RewriteError::new(error.code, error.message)
}

/// A rule that fails to compile: an unparseable pattern is `invalidPattern`
/// with astSearch's text; any other cause keeps its engine code, untagged.
fn rule_error(
    query: &RewriteRequest,
    extensions: &BTreeSet<String>,
    error: &str,
) -> ast_rule::RuleError {
    if let Some(pattern) = query.pattern()
        && let Err(invalid) = ast_rule::compile_check(extensions, Some(pattern), None)
    {
        return invalid;
    }
    let rewrite = engine_error(error.to_owned());
    ast_rule::RuleError {
        code: rewrite.code,
        message: rewrite.message,
    }
}

fn embedded_engine_receipt() -> ExecutableReceipt {
    ExecutableReceipt {
        path: PathBuf::from("native"),
        version: "embedded".to_owned(),
        sha256: String::new(),
        capability_digest: "native".to_owned(),
        capabilities: vec!["pattern".to_owned(), "inline-rules".to_owned()],
    }
}

/// Map an engine error by its leading `[code]` tag (the engine's typed error
/// prefix), never by text elsewhere in the message.
fn engine_error(error: String) -> RewriteError {
    let tag = error
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .map(|(tag, _)| tag);
    let code = match tag {
        Some("structural.rewrite.invalid" | "structural.rewrite.json") => {
            "ast.rewrite.input.invalid"
        }
        Some("structural.rewrite.matchLimit") => "ast.rewrite.match_limit",
        _ => "ast.rewrite.execution_failed",
    };
    RewriteError::new(code, ast_rule::untagged(&error))
}

/// Corpus-coverage accounting from the rewrite candidate scan. Mirrors the
/// engine's per-file skip counters so a preview/apply can report how much of the
/// tree the rewrite actually reached instead of dropping skipped files silently.
#[derive(Clone, Copy, Default)]
struct RewriteCoverage {
    scan_truncated: bool,
    skipped_unreadable: u32,
    skipped_large: u32,
    skipped_binary: u32,
    skipped_errored: u32,
}
impl RewriteCoverage {
    fn has_gaps(&self) -> bool {
        self.scan_truncated
            || self.skipped_unreadable > 0
            || self.skipped_large > 0
            || self.skipped_binary > 0
            || self.skipped_errored > 0
    }
    fn to_json(self) -> Value {
        json!({
            "scanTruncated": self.scan_truncated,
            "skippedUnreadable": self.skipped_unreadable,
            "skippedLarge": self.skipped_large,
            "skippedBinary": self.skipped_binary,
            "skippedErrored": self.skipped_errored,
        })
    }
    fn warning(&self) -> String {
        format!(
            "Rewrite coverage is partial: {} unreadable, {} oversized, {} non-UTF8, {} rewrite-errored file(s) were not covered{}. Results are a bounded subset of the corpus.",
            self.skipped_unreadable,
            self.skipped_large,
            self.skipped_binary,
            self.skipped_errored,
            if self.scan_truncated {
                ", and the candidate scan hit maxFiles"
            } else {
                ""
            }
        )
    }
}

/// Scanned matches, coverage, and each matched file's source syntax-error
/// count (from the scan's own parse).
type ScanOutput = (Vec<RawMatch>, RewriteCoverage, BTreeMap<String, u32>);

fn run_scan(
    query: &RewriteRequest,
    target: &Path,
    cancellation: &dyn CancellationCheck,
    analyzer: &StagedAnalyzer,
) -> Result<ScanOutput, RewriteError> {
    cancellation.check().map_err(cancelled)?;
    let config = analyzer.config();
    let max_files = u32::try_from(query.max_files()).map_err(|_| {
        RewriteError::new(
            "ast.rewrite.input.invalid",
            "maxFiles exceeds the native engine limit.",
        )
    })?;
    let files = octocode_engine::structural::rewrite_files(
        octocode_engine::structural::StructuralRewriteFilesOptions {
            path: target.to_string_lossy().into_owned(),
            rule_config_json: serde_json::to_string(config).map_err(|error| {
                RewriteError::new("ast.rewrite.input.invalid", error.to_string())
            })?,
            include: query.include().clone(),
            exclude: query.exclude().clone(),
            exclude_dir: Some(
                crate::policy::prune::PruneMode::SyntaxVisible
                    .directories(query.default_excludes()),
            ),
            hidden: Some(false),
            no_ignore: Some(false),
            max_depth: None,
            max_files: Some(max_files),
            max_file_bytes: Some(1_000_000),
        },
    )
    .map_err(engine_error)?;
    cancellation.check().map_err(cancelled)?;
    let coverage = RewriteCoverage {
        scan_truncated: files.scan_truncated,
        skipped_unreadable: files.skipped_unreadable,
        skipped_large: files.skipped_large,
        skipped_binary: files.skipped_binary,
        skipped_errored: files.skipped_errored,
    };

    note_parses(files.files.len());
    let mut raw = Vec::new();
    let mut source_errors = BTreeMap::new();
    for file in files.files {
        source_errors.insert(file.path.clone(), file.syntax_errors);
        for matched in file.matches {
            let mut meta_variables = RawMetaVariables::default();
            for (name, capture) in matched.captures {
                match capture.kind.as_str() {
                    "single" => {
                        meta_variables.single.insert(
                            name,
                            RawCapture {
                                text: capture.texts.first().cloned().unwrap_or_default(),
                            },
                        );
                    }
                    "multi" => {
                        meta_variables.multi.insert(
                            name,
                            capture
                                .texts
                                .into_iter()
                                .map(|text| RawCapture { text })
                                .collect(),
                        );
                    }
                    "transformed" => {
                        meta_variables
                            .transformed
                            .insert(name, capture.texts.first().cloned().unwrap_or_default());
                    }
                    _ => {}
                }
            }
            raw.push(RawMatch {
                file: file.path.clone(),
                range: RawRange {
                    byte_offset: RawByteRange {
                        start: matched.byte_start as usize,
                        end: matched.byte_end as usize,
                    },
                    start: RawPosition {
                        line: matched.range.start.line,
                        column: matched.range.start.column,
                    },
                    end: RawPosition {
                        line: matched.range.end.line,
                        column: matched.range.end.column,
                    },
                },
                text: matched.replaced_text,
                replacement: matched.replacement,
                replacement_offsets: None,
                meta_variables,
            });
        }
    }
    Ok((raw, coverage, source_errors))
}

fn prepare(
    query: &RewriteRequest,
    root: &Path,
    context: &PrepareContext<'_>,
) -> Result<(Vec<PreparedFile>, RewriteCoverage), RewriteError> {
    let (raw_matches, coverage, source_errors) =
        run_scan(query, root, context.cancellation, context.analyzer)?;
    if raw_matches.len() > query.max_matches() {
        return Err(RewriteError::new(
            "ast.rewrite.match_limit",
            format!(
                "The rewrite found {} matches, exceeding maxMatches={}. Narrow the scope.",
                raw_matches.len(),
                query.max_matches()
            ),
        )
        .detail(json!({"observed":raw_matches.len(),"maxMatches":query.max_matches()}))
        .terminal());
    }
    let grouped = group_targets(raw_matches, root, &source_errors, context)?;
    if grouped.len() > query.max_files() {
        return Err(RewriteError::new(
            "ast.rewrite.file_limit",
            format!(
                "The rewrite affects {} files, exceeding maxFiles={}.",
                grouped.len(),
                query.max_files()
            ),
        )
        .terminal());
    }
    let mut files = Vec::new();
    let mut total_patch_bytes = 0usize;
    for (absolute, (raw, scanned_errors)) in grouped {
        let file = prepare_file(absolute, raw, scanned_errors, context)?;
        total_patch_bytes = total_patch_bytes.saturating_add(file.patch.len());
        if total_patch_bytes > context.options.max_patch_bytes {
            return Err(RewriteError::new(
                "ast.rewrite.patch_limit",
                format!(
                    "Unified patches exceed the {}-byte response limit. Narrow the scope.",
                    context.options.max_patch_bytes
                ),
            )
            .terminal());
        }
        files.push(file);
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok((files, coverage))
}

/// Raw matches per target file, with the scan's syntax-error count.
type Targets = BTreeMap<PathBuf, (Vec<RawMatch>, Option<u32>)>;

/// Raw matches per verified target file (a regular file inside the
/// boundary, never a symlink), with the scan's error count for the file.
fn group_targets(
    raw_matches: Vec<RawMatch>,
    root: &Path,
    source_errors: &BTreeMap<String, u32>,
    context: &PrepareContext<'_>,
) -> Result<Targets, RewriteError> {
    let mut grouped = Targets::new();
    for matched in raw_matches {
        context.cancellation.check().map_err(cancelled)?;
        let unresolved = if Path::new(&matched.file).is_absolute() {
            PathBuf::from(&matched.file)
        } else {
            context.boundary.join(&matched.file)
        };
        let metadata = fs::symlink_metadata(&unresolved).map_err(|_| {
            RewriteError::new(
                "ast.rewrite.target_unavailable",
                "The native engine returned a target that could not be verified.",
            )
            .detail(json!({"path":matched.file}))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(RewriteError::new(
                "ast.rewrite.symlink_target",
                "The native engine returned a symlink or non-file target; no changes were prepared.",
            )
            .detail(json!({"path":matched.file})));
        }
        let target = context
            .paths
            .validate_read(&unresolved)
            .map_err(|error| RewriteError::new("ast.rewrite.path_escape", error.message))?;
        if !target.canonical.starts_with(context.boundary)
            || (root.is_file() && target.canonical != root)
        {
            return Err(RewriteError::new(
                "ast.rewrite.path_escape",
                "The native engine returned a target outside the real requested root.",
            )
            .detail(json!({"path":matched.file})));
        }
        let errors = source_errors.get(&matched.file).copied();
        let entry = grouped.entry(target.canonical).or_default();
        entry.1 = entry.1.or(errors);
        entry.0.push(matched);
    }
    Ok(grouped)
}

/// One target's staged edit: verified bytes, matches, the after content and
/// its syntax check, and the unified patch.
fn prepare_file(
    absolute: PathBuf,
    raw: Vec<RawMatch>,
    scanned_errors: Option<u32>,
    context: &PrepareContext<'_>,
) -> Result<PreparedFile, RewriteError> {
    context.cancellation.check().map_err(cancelled)?;
    let metadata = fs::symlink_metadata(&absolute).map_err(io_error)?;
    let before = fs::read(&absolute).map_err(io_error)?;
    context
        .security
        .validate_text_bytes(&before, Some(&absolute), MAX_FILE_BYTES)
        .map_err(|error| RewriteError::new("ast.rewrite.encoding_unsupported", error.message))?;
    let content = std::str::from_utf8(&before).map_err(|_| {
        RewriteError::new(
            "ast.rewrite.encoding_unsupported",
            "Only NUL-free UTF-8 source files can be rewritten.",
        )
    })?;
    let before_hash = sha256(&before);
    let relative = portable_relative(context.boundary, &absolute)?;
    let matches = prepare_matches(&relative, &before_hash, raw)?;
    let after = apply_edits(&before, &matches)?;
    let after_text = std::str::from_utf8(&after).map_err(|_| {
        RewriteError::new(
            "ast.rewrite.source_mismatch",
            "A generated replacement is not valid UTF-8.",
        )
    })?;
    let before_errors = match scanned_errors {
        Some(errors) => errors,
        None => context.analyzer.count_errors(&relative, content)?,
    };
    let after_facts =
        check_syntax_regression(context.analyzer, &relative, before_errors, after_text)?;
    let patch = create_unified_patch(&relative, content, after_text);
    Ok(PreparedFile {
        path: relative,
        absolute,
        before_hash,
        after_hash: sha256(&after),
        before,
        after,
        patch,
        matches,
        permissions: metadata.permissions(),
        before_errors,
        after_facts,
    })
}

fn rule_config(query: &RewriteRequest) -> Value {
    let mut config = Map::new();
    config.insert("id".to_owned(), json!("octocode-inline-rewrite"));
    config.insert("language".to_owned(), json!(query.lang()));
    config.insert("severity".to_owned(), json!("warning"));
    config.insert(
        "message".to_owned(),
        json!("Octocode inline structural rewrite"),
    );
    if let (Some(pattern), Some(rewrite)) = (query.pattern(), query.rewrite()) {
        config.insert("rule".to_owned(), json!({"pattern":pattern}));
        config.insert("fix".to_owned(), json!(rewrite));
    } else {
        let mut fields = query.rule_config_fields();
        for key in ["rule", "fix"] {
            config.insert(key.to_owned(), fields.remove(key).unwrap_or(Value::Null));
        }
        config.extend(fields);
    }
    Value::Object(config)
}

fn prepare_matches(
    path: &str,
    before_hash: &str,
    engine: Vec<RawMatch>,
) -> Result<Vec<PreparedMatch>, RewriteError> {
    let mut matches = engine
        .into_iter()
        .map(|matched| {
            let replacement_range = matched
                .replacement_offsets
                .as_ref()
                .unwrap_or(&matched.range.byte_offset);
            let (start, end) = (replacement_range.start, replacement_range.end);
            let mut captures = Map::new();
            for (name, value) in &matched.meta_variables.single {
                captures.insert(
                    name.clone(),
                    json!({"kind":"single","texts":[value.text.clone()]}),
                );
            }
            for (name, values) in &matched.meta_variables.multi {
                captures.insert(
                    name.clone(),
                    json!({
                        "kind":"multi",
                        "texts":values.iter().map(|value| value.text.clone()).collect::<Vec<_>>()
                    }),
                );
            }
            for (name, value) in &matched.meta_variables.transformed {
                captures.insert(name.clone(), json!({"kind":"transformed","texts":[value]}));
            }
            let id = json_sha256(&json!([
                path,
                before_hash,
                start,
                end,
                matched.text,
                matched.replacement
            ]));
            // Lines are 1-based like astSearch/localFetch; columns are 0-based
            // UTF-16 code units like astSearch and LSP. `byteRange` names the
            // replaced span and is emitted only when it differs from the
            // matched `range.byteOffset`.
            let range = &matched.range;
            let mut public = json!({
                "id":id,"path":path,
                "range":{
                    "byteOffset":range.byte_offset,
                    "start":{"line":range.start.line.saturating_add(1),"column":range.start.column},
                    "end":{"line":range.end.line.saturating_add(1),"column":range.end.column}
                },
                "text":matched.text,"replacement":matched.replacement,
                "captures":captures
            });
            if (start, end) != (range.byte_offset.start, range.byte_offset.end) {
                public["byteRange"] = json!({"start":start,"end":end});
            }
            PreparedMatch {
                public,
                id,
                start,
                end,
                expected: matched.text.into_bytes(),
                replacement: matched.replacement.into_bytes(),
            }
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|matched| (matched.start, matched.end));
    for pair in matches.windows(2) {
        if pair[1].start < pair[0].end
            || (pair[1].start == pair[0].start && pair[1].end == pair[0].end)
        {
            let nested = pair[1].end <= pair[0].end || pair[1].start == pair[0].start;
            return Err(RewriteError::new(
                "ast.rewrite.overlap",
                if nested {
                    "The rewrite pattern matched a node nested inside another match. Exclude the nesting so each match selects one non-overlapping syntax node, then preview again; no changes were prepared."
                } else {
                    "The rewrite pattern matched overlapping syntax ranges. Narrow the pattern so each match selects one non-overlapping syntax node, then preview again; no changes were prepared."
                },
            )
            .detail(json!({
                "path": path,
                "firstRange": {"start": pair[0].start, "end": pair[0].end},
                "secondRange": {"start": pair[1].start, "end": pair[1].end}
            })));
        }
    }
    Ok(matches)
}

/// Reject staged output that parses worse than its source. Byte-splicing
/// cleanly does not mean the result is valid code; comparing ERROR/MISSING
/// node counts keeps rewrites of already-broken files possible while blocking
/// templates that introduce new damage.
fn check_syntax_regression(
    analyzer: &StagedAnalyzer,
    path: &str,
    before_errors: u32,
    after: &str,
) -> Result<Option<StagedFacts>, RewriteError> {
    // Replacements can grow a near-limit file past the engine's parse bound;
    // an unverifiable-but-legal rewrite must stage rather than hard-fail.
    let Some(facts) = analyzer.staged(path, after)? else {
        return Ok(None);
    };
    let after_errors = facts.syntax_errors;
    if after_errors > before_errors {
        return Err(RewriteError::new(
            "ast.rewrite.broken_syntax",
            "The staged rewrite introduces new syntax errors; no files were changed. \
             Fix the rewrite template before retrying.",
        )
        .detail(json!({
            "path": path,
            "beforeErrorNodes": before_errors,
            "afterErrorNodes": after_errors,
        })));
    }
    Ok(Some(facts))
}

fn apply_edits(before: &[u8], matches: &[PreparedMatch]) -> Result<Vec<u8>, RewriteError> {
    let mut after = Vec::with_capacity(before.len());
    let mut offset = 0usize;
    for matched in matches {
        if matched.start < offset
            || matched.end < matched.start
            || matched.end > before.len()
            || before.get(matched.start..matched.end) != Some(matched.expected.as_slice())
        {
            return Err(RewriteError::new(
                "ast.rewrite.source_mismatch",
                "Rewrite match bytes did not agree with the verified source.",
            ));
        }
        after.extend_from_slice(&before[offset..matched.start]);
        after.extend_from_slice(&matched.replacement);
        offset = matched.end;
    }
    after.extend_from_slice(&before[offset..]);
    Ok(after)
}

fn select(
    query: &RewriteRequest,
    files: &[PreparedFile],
    matches: &[PreparedMatch],
    max_patch_bytes: usize,
    analyzer: &StagedAnalyzer,
) -> Result<(Vec<PreparedFile>, Vec<PreparedMatch>), RewriteError> {
    let Some(selected) = query.selected_match_ids() else {
        return Ok((files.to_vec(), matches.to_vec()));
    };
    // Each selector is a full id or a unique prefix (preview rows show 16
    // hex digits); one naming no match, or several, is rejected.
    let mut unknown = Vec::new();
    let mut ambiguous = Vec::new();
    let mut resolved = BTreeSet::new();
    for prefix in selected {
        let mut hits = matches
            .iter()
            .filter(|matched| matched.id.starts_with(&prefix));
        match (hits.next(), hits.next()) {
            (Some(matched), None) => {
                resolved.insert(matched.id.clone());
            }
            (None, _) => unknown.push(prefix),
            (Some(_), Some(_)) => ambiguous.push(prefix),
        }
    }
    if !unknown.is_empty() || !ambiguous.is_empty() {
        return Err(RewriteError::new(
            "ast.rewrite.selection_invalid",
            "selectedMatchIds must each name exactly one match of this snapshot; use more hex digits for an ambiguous prefix.",
        )
        .detail(json!({"unknown":unknown,"ambiguous":ambiguous})));
    }
    let selected = resolved;
    let mut selected_files = Vec::new();
    let mut selected_matches = Vec::new();
    let mut total_patch_bytes = 0usize;
    for file in files {
        let file_matches = file
            .matches
            .iter()
            .filter(|matched| selected.contains(&matched.id))
            .cloned()
            .collect::<Vec<_>>();
        if file_matches.is_empty() {
            continue;
        }
        let after = apply_edits(&file.before, &file_matches)?;
        let before_text = std::str::from_utf8(&file.before).map_err(|_| {
            RewriteError::new("ast.rewrite.encoding_unsupported", "Invalid UTF-8 source.")
        })?;
        let after_text = std::str::from_utf8(&after).map_err(|_| {
            RewriteError::new("ast.rewrite.source_mismatch", "Invalid UTF-8 replacement.")
        })?;
        // A subset of individually-clean edits can still break syntax (e.g.
        // dropping one of a paired open/close rewrite), so re-check here.
        let after_facts =
            check_syntax_regression(analyzer, &file.path, file.before_errors, after_text)?;
        let patch = create_unified_patch(&file.path, before_text, after_text);
        total_patch_bytes = total_patch_bytes.saturating_add(patch.len());
        if total_patch_bytes > max_patch_bytes {
            return Err(RewriteError::new(
                "ast.rewrite.patch_limit",
                "Selected patches exceed the response limit. Narrow the selection.",
            )
            .terminal());
        }
        let mut selected_file = file.clone();
        selected_file.after_hash = sha256(&after);
        selected_file.after = after;
        selected_file.after_facts = after_facts;
        selected_file.patch = patch;
        selected_file.matches = file_matches.clone();
        selected_matches.extend(file_matches);
        selected_files.push(selected_file);
    }
    Ok((selected_files, selected_matches))
}

fn validate_expected_hashes(
    query: &RewriteRequest,
    files: &[PreparedFile],
    boundary: &Path,
    paths: &PathPolicy,
) -> Result<(), RewriteError> {
    let mut expected = BTreeMap::new();
    for (path, hash) in query.expected_hashes().as_ref().into_iter().flatten() {
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(RewriteError::new(
                "ast.rewrite.expected_hash_invalid",
                "Expected hash values must be SHA-256 hex digests.",
            )
            .detail(json!({"path":path})));
        }
        // Preview reports boundary-relative paths; accept them back verbatim
        // alongside absolute paths.
        let absolute = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            boundary.join(path)
        };
        let validated = paths.validate_read(&absolute).map_err(|_| {
            RewriteError::new(
                "ast.rewrite.expected_hash_invalid",
                "An expected hash path could not be resolved.",
            )
            .detail(json!({"path":path}))
        })?;
        if !validated.canonical.starts_with(boundary) {
            return Err(RewriteError::new(
                "ast.rewrite.expected_hash_invalid",
                "An expected hash path is outside the real requested root.",
            ));
        }
        expected.insert(validated.canonical, hash.to_ascii_lowercase());
    }
    let actual = files
        .iter()
        .map(|file| file.absolute.clone())
        .collect::<BTreeSet<_>>();
    let expected_set = expected.keys().cloned().collect::<BTreeSet<_>>();
    if actual != expected_set {
        return Err(RewriteError::new(
            "ast.rewrite.expected_hash_set_mismatch",
            "Apply requires exactly the affected file paths returned by the matching preview.",
        )
        .detail(json!({"expectedPaths":expected_set,"actualPaths":actual})));
    }
    for file in files {
        let Some(hash) = expected.get(&file.absolute) else {
            return Err(RewriteError::new(
                "ast.rewrite.expected_hash_missing",
                "Apply requires the preview beforeHash for every affected absolute path.",
            ));
        };
        if hash != &file.before_hash {
            return Err(RewriteError::new(
                "ast.rewrite.hash_mismatch",
                "An expected source hash no longer matches; preview again before applying.",
            )
            .detail(json!({"path":file.absolute,"expected":hash,"actual":file.before_hash})));
        }
    }
    Ok(())
}

fn validate_postconditions(
    query: &RewriteRequest,
    files: &[PreparedFile],
    cancellation: &dyn CancellationCheck,
) -> Result<(), RewriteError> {
    let Some(postconditions) = query.postconditions() else {
        return Ok(());
    };
    if postconditions.is_empty() {
        return Ok(());
    }
    // The staged tree was already parsed by the syntax-regression check.
    let mut remaining = 0usize;
    for file in files {
        cancellation.check().map_err(cancelled)?;
        let observed = file
            .after_facts
            .and_then(|facts| facts.remaining)
            .ok_or_else(|| {
                RewriteError::new(
                    "ast.rewrite.postcondition_execution_failed",
                    "The postcondition scan could not be completed; no files were changed.",
                )
            })?;
        remaining = remaining.saturating_add(observed);
    }
    for postcondition in postconditions {
        // `kind` is the single contract value `remainingMatches`.
        if usize::try_from(postcondition.equals).ok() != Some(remaining) {
            return Err(RewriteError::new(
                "ast.rewrite.postcondition_failed",
                "A staged rewrite postcondition failed; no files were changed.",
            )
            .detail(json!({
                "kind":postcondition.kind,"expected":postcondition.equals,"observed":remaining,
                "scope":"rewrittenFiles",
                "scannedFiles":files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>()
            })));
        }
    }
    Ok(())
}

fn snapshot(
    query: &RewriteRequest,
    root: &Path,
    files: &[PreparedFile],
    executable: &ExecutableReceipt,
) -> String {
    let ids = files
        .iter()
        .flat_map(|file| file.matches.iter().map(|matched| matched.id.clone()))
        .collect::<Vec<_>>();
    let file_hashes = files
        .iter()
        .map(|file| json!([file.absolute, file.before_hash, file.after_hash]))
        .collect::<Vec<_>>();
    let rule_spec = if query.pattern().is_some() {
        json!({
            "ruleKind":"pattern",
            "pattern":query.pattern(),
            "rewrite":query.rewrite()
        })
    } else {
        let mut spec = query.rule_config_fields();
        spec.insert("ruleKind".to_owned(), json!("rule"));
        spec.entry("rule").or_insert(Value::Null);
        Value::Object(spec)
    };
    json_sha256(&json!({
        "contract":1,
        "executable":{
            "path":executable.path,
            "version":executable.version,
            "sha256":executable.sha256,
            "capabilityDigest":executable.capability_digest
        },
        "root":root,
        "language":query.lang(),
        "ruleSpec":rule_spec,
        "include":query.include().as_deref().unwrap_or(&[]),
        "exclude":query.exclude().as_deref().unwrap_or(&[]),
        "maxFiles":query.max_files(),
        "maxMatches":query.max_matches(),
        "files":file_hashes,
        "matchIds":ids
    }))
}

fn transaction_id(root: &Path, files: &[PreparedFile]) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    json_sha256(&json!([
        root,
        std::process::id(),
        now.to_string(),
        files
            .iter()
            .map(|file| &file.before_hash)
            .collect::<Vec<_>>()
    ]))
}

fn cancelled(message: String) -> RewriteError {
    RewriteError::new("ast.rewrite.cancelled", message)
}

fn io_error(error: std::io::Error) -> RewriteError {
    RewriteError::new("ast.rewrite.io", error.to_string())
}

/// Base directory for astRewrite's cross-process lock and journal state.
///
/// Defaults to the shared system temp dir so real, concurrent octocode
/// processes serialize against the *same* lock and honor each other's journals
/// — the safety guarantee must not be namespaced away. `OCTOCODE_AST_REWRITE_STATE_DIR`
/// redirects it for sandboxed/hardened deployments where the system temp dir is
/// not writable, and lets CI point contending test binaries at private roots
/// instead of fighting over the one global lock.
pub(super) fn state_base_dir() -> PathBuf {
    std::env::var_os("OCTOCODE_AST_REWRITE_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// Per-user suffix for the shared lock/journal directory names under the
/// world-writable temp root. Namespacing by uid keeps two legitimate users from
/// colliding on one predictable directory — without it, whichever user creates
/// `octocode-ast-rewrite-*-v1` first owns it and every other user's astRewrite
/// then fails the ownership check in `create_private_dir_all` (a fail-closed
/// denial of service). On non-unix a fixed label is used.
pub(super) fn state_dir_uid_suffix() -> String {
    #[cfg(unix)]
    {
        // SAFETY: getuid() takes no arguments, has no preconditions, cannot fail,
        // and only reads the caller's real user id.
        let uid = unsafe { libc::getuid() };
        format!("uid-{uid}")
    }
    #[cfg(not(unix))]
    {
        "shared".to_owned()
    }
}

/// Create `path` (and any missing parents) with owner-only `0700` permissions on
/// unix, then verify the resulting directory is owned by the current uid. The
/// journal and lock roots live under `std::env::temp_dir()`, a shared,
/// world-writable location; this prevents another local user from pre-creating a
/// predictable directory and racing on its contents or planting journals/locks.
fn create_private_dir_all(path: &Path) -> Result<(), RewriteError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(io_error)?;
        let metadata = fs::metadata(path).map_err(io_error)?;
        // SAFETY: getuid() takes no arguments, has no preconditions, cannot fail,
        // and only reads the caller's real user id.
        let current_uid = unsafe { libc::getuid() };
        if metadata.uid() != current_uid {
            return Err(RewriteError::new(
                "ast.rewrite.io",
                format!(
                    "Refusing to use a directory owned by another user: {}",
                    path.display()
                ),
            ));
        }
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path).map_err(io_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
