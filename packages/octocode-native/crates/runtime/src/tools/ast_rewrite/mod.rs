use crate::tools::id::ToolId;
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::cancel::CancellationCheck,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

mod journal;
mod lock;
mod output;

use journal::{commit_transaction, recover_transactions};
use lock::RootLock;
use output::{attach_receipts, continuation_query, portable_relative, success_value};
mod raw;
mod staged;
use raw::{RawByteRange, RawCapture, RawMatch, RawMetaVariables, RawPosition, RawRange};
use staged::{StagedAnalyzer, StagedFacts, note_parses};

/// Contract default of an omitted bound (validation normally stamps it); a
/// contract without that default fails closed to the smallest bound.
fn contract_default(field: &str) -> usize {
    crate::contracts::query_schema_number(ToolId::AstRewrite, None, field, "default")
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(1)
}
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

pub use crate::contracts::tool_types::{
    ArPostconditionsItem, AstRewriteQuery, AstRewriteQueryPattern, AstRewriteQueryRule,
};

/// Binds `$field` from either rule kind of `$query`.
macro_rules! either_kind {
    ($query:expr, $field:ident => $value:expr) => {
        match $query {
            AstRewriteQuery::Pattern(AstRewriteQueryPattern { $field, .. })
            | AstRewriteQuery::Rule(AstRewriteQueryRule { $field, .. }) => $value,
        }
    };
}

fn usize_of(value: std::num::NonZeroU64) -> usize {
    usize::try_from(value.get()).unwrap_or(usize::MAX)
}

/// Rule-kind-independent views over the generated wire query, with the
/// runtime's defaults for the optional bounds.
impl AstRewriteQuery {
    pub fn rule_kind(&self) -> &'static str {
        match self {
            Self::Pattern(_) => "pattern",
            Self::Rule(_) => "rule",
        }
    }
    pub fn path(&self) -> &str {
        either_kind!(self, path => path.as_str())
    }
    /// The requested parser; `None` asks the runtime to infer it.
    pub fn lang_type(&self) -> Option<&str> {
        either_kind!(self, lang_type => lang_type.as_ref().map(|language| language.as_str()))
    }
    pub fn pattern(&self) -> Option<&str> {
        match self {
            Self::Pattern(query) => Some(query.pattern.as_str()),
            Self::Rule(_) => None,
        }
    }
    pub fn rewrite(&self) -> Option<&str> {
        match self {
            Self::Pattern(query) => Some(query.rewrite.as_str()),
            Self::Rule(_) => None,
        }
    }
    pub fn default_excludes(&self) -> bool {
        use crate::policy::prune::DefaultsFlag;
        either_kind!(self, default_excludes => default_excludes.defaults())
    }
    pub fn include(&self) -> Option<Vec<String>> {
        either_kind!(self, include => include
            .as_ref()
            .map(|globs| globs.iter().map(ToString::to_string).collect()))
    }
    pub fn exclude(&self) -> Option<Vec<String>> {
        either_kind!(self, exclude => exclude
            .as_ref()
            .map(|globs| globs.iter().map(ToString::to_string).collect()))
    }
    pub fn apply(&self) -> bool {
        either_kind!(self, apply => apply.as_ref().is_some_and(|apply| apply.0))
    }
    pub fn debug(&self) -> bool {
        either_kind!(self, debug => *debug)
    }
    /// Expected pre-image hashes, in path order.
    pub fn expected_hashes(&self) -> Option<BTreeMap<String, String>> {
        either_kind!(self, expected_hashes => expected_hashes.as_ref().map(|hashes| {
            hashes
                .iter()
                .map(|(path, hash)| (path.to_string(), hash.to_string()))
                .collect()
        }))
    }
    pub fn selected_match_ids(&self) -> Option<Vec<String>> {
        either_kind!(self, selected_match_ids => selected_match_ids
            .as_ref()
            .map(|ids| ids.iter().map(ToString::to_string).collect()))
    }
    pub fn postconditions(&self) -> Option<&[ArPostconditionsItem]> {
        either_kind!(self, postconditions => postconditions.as_deref().map(Vec::as_slice))
    }
    pub fn max_files(&self) -> usize {
        either_kind!(self, max_files => max_files.as_ref().map_or_else(|| contract_default("maxFiles"), |n| usize_of(n.0)))
    }
    pub fn max_matches(&self) -> usize {
        either_kind!(self, max_matches => max_matches.as_ref().map_or_else(|| contract_default("maxMatches"), |n| usize_of(n.0)))
    }
    pub fn page(&self) -> usize {
        either_kind!(self, page => page.as_ref().map_or_else(|| contract_default("page"), |n| usize_of(n.0)))
    }
    pub fn page_size(&self) -> usize {
        either_kind!(self, page_size => page_size.as_ref().map_or_else(|| contract_default("pageSize"), |n| usize_of(n.0)))
    }
    pub fn snapshot(&self) -> Option<&str> {
        either_kind!(self, snapshot => snapshot.as_ref().map(|snapshot| snapshot.0.as_str()))
    }
}

/// A parsed astRewrite row. The runtime parses it with its shared
/// `parse_query`, so a shape mismatch has one code across tools.
#[derive(Clone, Debug)]
pub struct RewriteRequest {
    query: AstRewriteQuery,
    /// The validated row as received. Security validation reads it, and the
    /// ast-grep rule JSON (`rule`, `constraints`, `utils`, `transform`, `fix`)
    /// is forwarded from it verbatim: typify models those record fields as
    /// `HashMap`s, whose iteration order would make the rule config and the
    /// preview snapshot digest nondeterministic. A YAML-string `rule` is
    /// parsed into the same object here, so both shapes preview identically.
    row: Value,
    /// Parser inferred when `langType` is omitted (set before any scan).
    inferred_lang: Option<String>,
}

impl<'de> Deserialize<'de> for RewriteRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut row = Value::deserialize(deserializer)?;
        let query = serde_json::from_value(row.clone()).map_err(serde::de::Error::custom)?;
        if let Some(Value::String(text)) = row.get("rule") {
            row["rule"] = rule_from_yaml(text).map_err(serde::de::Error::custom)?;
        }
        Ok(Self {
            query,
            row,
            inferred_lang: None,
        })
    }
}

/// Rule-file metadata a pasted ast-grep rule document may carry; none of it
/// affects matching (`langType` selects the parser).
const RULE_FILE_METADATA: &[&str] = &[
    "id", "language", "severity", "message", "note", "url", "metadata",
];

/// A YAML rule string as the equivalent rule object: a bare rule, or a rule
/// file whose `rule:` is used (its metadata keys are ignored).
fn rule_from_yaml(text: &str) -> Result<Value, String> {
    let value: Value =
        serde_yaml_ng::from_str(text).map_err(|error| format!("invalid rule YAML: {error}"))?;
    let Some(object) = value.as_object() else {
        return Err("rule YAML must be a mapping of ast-grep rule keys".to_owned());
    };
    let Some(rule) = object.get("rule") else {
        return Ok(value);
    };
    if let Some(key) = object
        .keys()
        .find(|key| *key != "rule" && !RULE_FILE_METADATA.contains(&key.as_str()))
    {
        return Err(format!(
            "rule file key `{key}` is not read from the rule string; pass it as its own astRewrite field"
        ));
    }
    Ok(rule.clone())
}

impl RewriteRequest {
    /// The parser this request runs with: `langType`, else the inferred one.
    pub fn lang(&self) -> &str {
        self.query
            .lang_type()
            .or(self.inferred_lang.as_deref())
            .unwrap_or_default()
    }
    fn rule_field(&self, key: &str) -> Option<&Value> {
        (self.query.rule_kind() == "rule")
            .then(|| self.row.get(key))
            .flatten()
    }
    pub fn rule(&self) -> Option<&Value> {
        self.rule_field("rule")
    }
    pub fn constraints(&self) -> Option<&Value> {
        self.rule_field("constraints")
    }
    pub fn utils(&self) -> Option<&Value> {
        self.rule_field("utils")
    }
    pub fn transform(&self) -> Option<&Value> {
        self.rule_field("transform")
    }
    pub fn fix(&self) -> Option<&Value> {
        self.rule_field("fix")
    }
}

impl std::ops::Deref for RewriteRequest {
    type Target = AstRewriteQuery;
    fn deref(&self) -> &AstRewriteQuery {
        &self.query
    }
}

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
    phase: String,
    files: Vec<JournalFile>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct JournalFile {
    target: PathBuf,
    stage: PathBuf,
    backup: PathBuf,
    before_hash: String,
    after_hash: String,
    state: String,
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
        self.next = Some(Box::new(json!({"restart":{
            "tool":ToolId::AstRewrite.as_str(),"query":restart,"confidence":"exact"
        }})));
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
            value["complete"] = json!(false);
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
            "Run the complete preview's next.apply unchanged; it carries snapshot and expectedHashes."
        }
        "ast.rewrite.expected_hash_invalid" => {
            "Key expectedHashes by preview files[].path with its 64-hex beforeHash, or run next.apply unchanged."
        }
        "ast.rewrite.expected_hash_set_mismatch" | "ast.rewrite.expected_hash_missing" => {
            "expectedHashes must list exactly the preview's affected files; run the complete preview's next.apply unchanged."
        }
        "ast.rewrite.hash_mismatch" => {
            "A file changed since preview; follow next.restart, then apply with the new preview's next.apply."
        }
        "ast.rewrite.source_mismatch" => {
            "Match bytes or the generated replacement disagree with the verified source; preview again with the same query."
        }
        "ast.rewrite.snapshot_changed" => {
            "Discard earlier preview pages; follow next.restart and page the new preview to its next.apply."
        }
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
    let checked = security.validate_input_parameters(&query.row);
    if !checked.is_valid {
        return Err(RewriteError::new(
            "ast.rewrite.security_validation_failed",
            checked.warnings.join("; "),
        ));
    }
    validate_query(&query)?;
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
    if query.lang_type().is_none() {
        query.inferred_lang = Some(infer_language(
            &query,
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
    // Recover from a poisoned lock (a prior panic while holding it) instead of
    // permanently bricking astRewrite for the rest of the process lifetime.
    let _process_guard = APPLY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let lock = RootLock::acquire(&boundary)?;
    recover_transactions(&boundary, cancellation)?;
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
        drop(lock);
        return Err(RewriteError::new(
            "ast.rewrite.snapshot_changed",
            "The source, executable, query, or selected file set changed. Preview again before continuing.",
        )
        .detail(json!({"snapshot":snapshot}))
        .restart(&query));
    }
    if prepared.is_empty() {
        drop(lock);
        let mut empty = json!({
            "status":"empty","operation":"rewrite",
            "mode":if query.apply() {"apply"} else {"preview"},
            "root":root,
            "totalMatches":0,"affectedFiles":0,"matches":[],"files":[],
            "complete":!coverage.has_gaps(),"isPartial":coverage.has_gaps()
        });
        attach_receipts(&mut empty, &query, &executable);
        if coverage.has_gaps() {
            empty["coverage"] = coverage.to_json();
            empty["warnings"] = json!([coverage.warning()]);
        }
        return Ok(empty);
    }
    let all_matches = prepared
        .iter()
        .flat_map(|file| file.matches.iter().cloned())
        .collect::<Vec<_>>();
    let (result_files, result_matches) = if query.apply() {
        select(
            &query,
            &prepared,
            &all_matches,
            options.max_patch_bytes,
            &analyzer,
        )?
    } else {
        (prepared.clone(), all_matches)
    };
    let transaction = if query.apply() {
        validate_expected_hashes(&query, &result_files, &boundary, paths).map_err(|error| {
            if error.code == "ast.rewrite.hash_mismatch" {
                error.restart(&query)
            } else {
                error
            }
        })?;
        validate_postconditions(&query, &result_files, cancellation)?;
        Some(commit_transaction(&boundary, &result_files, cancellation)?)
    } else {
        None
    };
    drop(lock);
    let mut value = success_value(
        &query,
        &root,
        &snapshot,
        &result_files,
        &result_matches,
        &executable,
        transaction,
    );
    if coverage.has_gaps() {
        value["coverage"] = coverage.to_json();
        if let Some(warnings) = value.get_mut("warnings").and_then(Value::as_array_mut) {
            warnings.push(json!(coverage.warning()));
        } else {
            value["warnings"] = json!([coverage.warning()]);
        }
    }
    Ok(value)
}

fn validate_query(query: &RewriteRequest) -> Result<(), RewriteError> {
    if query.path().trim().is_empty()
        || query
            .lang_type()
            .is_some_and(|language| language.trim().is_empty())
    {
        return Err(RewriteError::new(
            "ast.rewrite.input.invalid",
            "path and langType must not be blank.",
        ));
    }
    if query.page_size() > 1_000 {
        return Err(RewriteError::new(
            "ast.rewrite.pagination_invalid",
            "page and pageSize must be positive integers and pageSize must not exceed 1000.",
        ));
    }
    if query.max_files() > 50_000 {
        return Err(RewriteError::new(
            "ast.rewrite.input.invalid",
            "maxFiles must be between 1 and 50000.",
        ));
    }
    if query.max_matches() > 100_000 {
        return Err(RewriteError::new(
            "ast.rewrite.input.invalid",
            "maxMatches must be between 1 and 100000.",
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
            "The selected ruleKind is missing its required rewrite fields.",
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

/// The parser for a query without `langType`: a file's extension, or the one
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
        let prune = crate::policy::prune::PruneMode::SyntaxVisible
            .directories(&[], query.default_excludes());
        crate::tools::ast_search::present_grammars(
            root,
            &prune,
            false,
            false,
            query.max_files(),
            &|path| {
                cancellation.check()?;
                Ok(paths.permits_discovery(path))
            },
        )
        .map_err(required)?
        .into_keys()
        .collect::<Vec<_>>()
    } else {
        let extension = root
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        octocode_engine::portable::grammar_capabilities()
            .into_iter()
            .filter(|capability| capability.structural_search)
            .find(|capability| capability.extensions.contains(&extension))
            .map(|capability| vec![crate::tools::ast_search::grammar_selector(&capability)])
            .unwrap_or_default()
    };
    let compiles = candidates
        .into_iter()
        .filter(|language| {
            let mut probe = query.clone();
            probe.inferred_lang = Some(language.clone());
            octocode_engine::structural::compile_rewrite(rule_config(&probe)).is_ok()
        })
        .collect::<Vec<_>>();
    match compiles.as_slice() {
        [language] => Ok(language.clone()),
        [] => Err(required(
            "No supported grammar under path compiles this rule; set langType.".to_owned(),
        )),
        several => Err(required(format!(
            "Several grammars under path compile this rule ({}); set langType to one of them.",
            several.join(", ")
        ))),
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
    RewriteError::new(code, error)
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
                    .directories(&[], query.default_excludes()),
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
    let mut grouped = BTreeMap::<PathBuf, (Vec<RawMatch>, Option<u32>)>::new();
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
        context.cancellation.check().map_err(cancelled)?;
        let metadata = fs::symlink_metadata(&absolute).map_err(io_error)?;
        let before = fs::read(&absolute).map_err(io_error)?;
        context
            .security
            .validate_text_bytes(&before, Some(&absolute), MAX_FILE_BYTES)
            .map_err(|error| {
                RewriteError::new("ast.rewrite.encoding_unsupported", error.message)
            })?;
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
        total_patch_bytes = total_patch_bytes.saturating_add(patch.len());
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
        files.push(PreparedFile {
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
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok((files, coverage))
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
    if query.rule_kind() == "pattern" {
        config.insert("rule".to_owned(), json!({"pattern":query.pattern()}));
        config.insert("fix".to_owned(), json!(query.rewrite()));
    } else {
        config.insert(
            "rule".to_owned(),
            query.rule().cloned().unwrap_or(Value::Null),
        );
        config.insert(
            "fix".to_owned(),
            query.fix().cloned().unwrap_or(Value::Null),
        );
        for (key, value) in [
            ("constraints", query.constraints()),
            ("utils", query.utils()),
            ("transform", query.transform()),
        ] {
            if let Some(value) = value {
                config.insert(key.to_owned(), value.clone());
            }
        }
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
            let id = sha256(
                serde_json::to_vec(&json!([
                    path,
                    before_hash,
                    start,
                    end,
                    matched.text,
                    matched.replacement
                ]))
                .unwrap_or_default(),
            );
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
    let rule_spec = if query.rule_kind() == "rule" {
        let mut spec = Map::new();
        spec.insert("ruleKind".to_owned(), json!(query.rule_kind()));
        spec.insert(
            "rule".to_owned(),
            query.rule().cloned().unwrap_or(Value::Null),
        );
        for (key, value) in [
            ("constraints", query.constraints()),
            ("utils", query.utils()),
            ("transform", query.transform()),
            ("fix", query.fix()),
        ] {
            if let Some(value) = value {
                spec.insert(key.to_owned(), value.clone());
            }
        }
        Value::Object(spec)
    } else {
        json!({
            "ruleKind":"pattern",
            "pattern":query.pattern(),
            "rewrite":query.rewrite()
        })
    };
    sha256(
        serde_json::to_vec(&json!({
            "contract":1,
            "executable":{
                "path":executable.path,
                "version":executable.version,
                "sha256":executable.sha256,
                "capabilityDigest":executable.capability_digest
            },
            "root":root,
            "langType":query.lang(),
            "ruleSpec":rule_spec,
            "include":query.include().as_deref().unwrap_or(&[]),
            "exclude":query.exclude().as_deref().unwrap_or(&[]),
            "maxFiles":query.max_files(),
            "maxMatches":query.max_matches(),
            "pageSize":query.page_size(),
            "files":file_hashes,
            "matchIds":ids
        }))
        .unwrap_or_default(),
    )
}

fn transaction_id(root: &Path, files: &[PreparedFile]) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    sha256(
        serde_json::to_vec(&json!([
            root,
            std::process::id(),
            now.to_string(),
            files
                .iter()
                .map(|file| &file.before_hash)
                .collect::<Vec<_>>()
        ]))
        .unwrap_or_default(),
    )
}

fn sha256(value: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(value.as_ref()))
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

/// Unchanged lines shown around each hunk of the preview patch.
const PATCH_CONTEXT: usize = 3;
/// Edit-distance budget for the line diff. Past it the changed region is shown
/// as one replaced block: still a correct patch, just not minimal.
const PATCH_MAX_EDITS: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LineOp {
    Equal,
    Delete,
    Insert,
}

/// Myers shortest edit script over lines. `None` when the script needs more
/// than `max_edits` insertions plus deletions. Memory is O(D²) in the edit
/// count, not O(N·M) in the file size.
fn line_diff(old: &[&str], new: &[&str], max_edits: usize) -> Option<Vec<LineOp>> {
    let (n, m) = (old.len() as isize, new.len() as isize);
    let limit = (old.len() + new.len()).min(max_edits) as isize;
    let offset = limit + 1;
    let mut v = vec![0isize; (2 * offset + 1) as usize];
    // trace[d] holds v[-(d+1)..=(d+1)] as it was before step d.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut found = false;
    'search: for d in 0..=limit {
        trace.push(v[(offset - d - 1) as usize..=(offset + d + 1) as usize].to_vec());
        let mut k = -d;
        while k <= d {
            let at = |k: isize| (offset + k) as usize;
            let mut x = if k == -d || (k != d && v[at(k - 1)] < v[at(k + 1)]) {
                v[at(k + 1)]
            } else {
                v[at(k - 1)] + 1
            };
            let mut y = x - k;
            while x < n && y < m && old[x as usize] == new[y as usize] {
                x += 1;
                y += 1;
            }
            v[at(k)] = x;
            if x >= n && y >= m {
                found = true;
                break 'search;
            }
            k += 2;
        }
    }
    if !found {
        return None;
    }
    let mut ops = Vec::with_capacity(old.len() + new.len());
    let (mut x, mut y) = (n, m);
    for (d, snapshot) in trace.iter().enumerate().rev() {
        let d = d as isize;
        let get = |k: isize| snapshot[(k + d + 1) as usize];
        let k = x - y;
        let prev_k = if k == -d || (k != d && get(k - 1) < get(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = get(prev_k);
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            ops.push(LineOp::Equal);
            x -= 1;
            y -= 1;
        }
        if d > 0 {
            ops.push(if x == prev_x {
                LineOp::Insert
            } else {
                LineOp::Delete
            });
            x = prev_x;
            y = prev_y;
        }
    }
    ops.reverse();
    Some(ops)
}

fn create_unified_patch(path: &str, before: &str, after: &str) -> String {
    if before == after {
        return String::new();
    }
    // Split preserving line endings so the preview reflects CRLF and missing
    // final newlines faithfully; `str::lines()` would drop that information and
    // produce hunk bodies that disagree with the bytes on disk. This is a
    // display-only preview; the actual apply is a byte splice elsewhere.
    let old = before.split_inclusive('\n').collect::<Vec<_>>();
    let new = after.split_inclusive('\n').collect::<Vec<_>>();
    let mut prefix = 0usize;
    while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < old.len().saturating_sub(prefix)
        && suffix < new.len().saturating_sub(prefix)
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let old_mid = &old[prefix..old.len() - suffix];
    let new_mid = &new[prefix..new.len() - suffix];
    let middle = line_diff(old_mid, new_mid, PATCH_MAX_EDITS).unwrap_or_else(|| {
        let mut block = vec![LineOp::Delete; old_mid.len()];
        block.extend(std::iter::repeat_n(LineOp::Insert, new_mid.len()));
        block
    });
    // Whole-file script, each op with the old/new line index it consumes.
    let mut ops = Vec::with_capacity(prefix + middle.len() + suffix);
    let (mut oi, mut ni) = (0usize, 0usize);
    for op in std::iter::repeat_n(LineOp::Equal, prefix)
        .chain(middle)
        .chain(std::iter::repeat_n(LineOp::Equal, suffix))
    {
        ops.push((op, oi, ni));
        match op {
            LineOp::Equal => {
                oi += 1;
                ni += 1;
            }
            LineOp::Delete => oi += 1,
            LineOp::Insert => ni += 1,
        }
    }
    // Group changes whose unchanged gap fits inside two contexts into one hunk.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for (index, (op, _, _)) in ops.iter().enumerate() {
        if *op == LineOp::Equal {
            continue;
        }
        match groups.last_mut() {
            Some(group) if index - group.1 <= 2 * PATCH_CONTEXT + 1 => group.1 = index,
            _ => groups.push((index, index)),
        }
    }
    let mut patch = String::new();
    patch.push_str(&format!("--- a/{path}\n"));
    patch.push_str(&format!("+++ b/{path}\n"));
    for (first, last) in groups {
        let lo = first.saturating_sub(PATCH_CONTEXT);
        let hi = (last + PATCH_CONTEXT).min(ops.len() - 1);
        let hunk = &ops[lo..=hi];
        let old_count = hunk.iter().filter(|(op, ..)| *op != LineOp::Insert).count();
        let new_count = hunk.iter().filter(|(op, ..)| *op != LineOp::Delete).count();
        // An empty side names the line before the hunk (unified-diff rule).
        let start = |index: usize, count: usize| if count == 0 { index } else { index + 1 };
        patch.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            start(hunk[0].1, old_count),
            old_count,
            start(hunk[0].2, new_count),
            new_count
        ));
        for &(op, oi, ni) in hunk {
            let (marker, line) = match op {
                LineOp::Equal => (' ', old[oi]),
                LineOp::Delete => ('-', old[oi]),
                LineOp::Insert => ('+', new[ni]),
            };
            patch.push(marker);
            patch.push_str(line);
            if !line.ends_with('\n') {
                // A line without a trailing newline (final line of a no-EOF-newline
                // file) still needs to terminate the diff row it lives on.
                patch.push('\n');
                patch.push_str("\\ No newline at end of file\n");
            }
        }
    }
    patch
}

#[cfg(test)]
mod tests {
    use super::journal::{journal_directory, persist_journal};
    use super::*;
    use crate::policy::path::PathPolicyConfig;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Tests speak JSON rows; the runtime owns the typed parse.
    fn rewrite_row(
        query: Value,
        paths: &PathPolicy,
        security: &ContentSecurity,
        cancellation: &dyn CancellationCheck,
        options: &AstRewriteRuntimeOptions,
    ) -> Value {
        let mut query = query;
        for field in ["goal", "reasoning"] {
            if query[field].is_null() {
                query[field] = Value::from("test");
            }
        }
        let query: RewriteRequest = serde_json::from_value(query).expect("typed astRewrite row");
        execute_ast_rewrite_with_options(query, paths, security, cancellation, options)
    }

    #[test]
    fn overlap_error_identifies_ranges_and_how_to_narrow_the_preview() {
        let matched = |start, end| RawMatch {
            file: "a.ts".into(),
            text: "matched".into(),
            replacement: "replaced".into(),
            range: RawRange {
                byte_offset: RawByteRange { start, end },
                start: RawPosition {
                    line: 0,
                    column: start as u32,
                },
                end: RawPosition {
                    line: 0,
                    column: end as u32,
                },
            },
            replacement_offsets: None,
            meta_variables: RawMetaVariables::default(),
        };
        let error = prepare_matches("a.ts", "before", vec![matched(0, 12), matched(5, 14)])
            .expect_err("overlap must reject preview");
        let value = error.value();
        assert_eq!(value["errorCode"], "ast.rewrite.overlap");
        assert!(
            value["error"].as_str().is_some_and(|message| {
                message.contains("Narrow the pattern") && message.contains("preview again")
            }),
            "{value}"
        );
        // The message and the hint agree: both narrow, neither broadens.
        let hint = value["hints"][0].as_str().expect("overlap hint");
        assert!(
            hint.contains("not:{has:") && !hint.contains("Broaden"),
            "{value}"
        );
        // Whole within the response guidance cap, so it is never clipped.
        assert!(hint.chars().count() <= 120, "{hint}");
        let nested = prepare_matches("a.ts", "before", vec![matched(0, 12), matched(0, 5)])
            .expect_err("nested matches reject preview")
            .value();
        assert!(
            nested["error"]
                .as_str()
                .is_some_and(|message| message.contains("nested inside another match")),
            "{nested}"
        );
        assert_eq!(
            value["details"]["firstRange"],
            json!({"start": 0, "end": 12})
        );
        assert_eq!(
            value["details"]["secondRange"],
            json!({"start": 5, "end": 14})
        );
    }

    #[test]
    fn unified_patch_preserves_crlf_line_endings() {
        let patch = create_unified_patch("f.txt", "a\r\nb\r\n", "a\r\nB\r\n");
        // The CRLF endings from the source must survive into the hunk body.
        assert!(patch.contains("-b\r\n"), "patch was: {patch:?}");
        assert!(patch.contains("+B\r\n"), "patch was: {patch:?}");
        assert!(patch.starts_with("--- a/f.txt\n+++ b/f.txt\n@@ "));
    }

    #[test]
    fn unified_patch_is_minimal_with_separate_hunks_and_context() {
        let before = "fn a() {\n    fill_goal(next, goal);\n    x();\n    y();\n    z();\n    w();\n    v();\n    u();\n    t();\n    fill_goal(next, goal);\n}\n";
        let after = before.replace("fill_goal(", "fill_goal2(");
        let patch = create_unified_patch("a.rs", before, &after);
        assert_eq!(
            patch,
            "--- a/a.rs\n+++ b/a.rs\n\
             @@ -1,5 +1,5 @@\n fn a() {\n-    fill_goal(next, goal);\n+    fill_goal2(next, goal);\n     x();\n     y();\n     z();\n\
             @@ -7,5 +7,5 @@\n     v();\n     u();\n     t();\n-    fill_goal(next, goal);\n+    fill_goal2(next, goal);\n }\n"
        );
    }

    #[test]
    fn unified_patch_merges_near_changes_and_counts_pure_insertions() {
        // Changes 6 lines apart share one hunk: only the two changed lines are
        // marked, the lines between them are context.
        let before = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let after = "A\nb\nc\nd\ne\nf\ng\nH\n";
        let patch = create_unified_patch("f", before, after);
        assert!(
            patch.contains("@@ -1,8 +1,8 @@\n-a\n+A\n b\n c\n d\n e\n f\n g\n-h\n+H\n"),
            "{patch}"
        );
        assert_eq!(patch.matches("@@ -").count(), 1, "{patch}");
        let inserted = create_unified_patch("f", "a\nb\n", "a\nx\nb\n");
        assert!(
            inserted.contains("@@ -1,2 +1,3 @@\n a\n+x\n b\n"),
            "{inserted}"
        );
        let appended = create_unified_patch("f", "", "x\n");
        assert!(appended.contains("@@ -0,0 +1,1 @@\n+x\n"), "{appended}");
    }

    #[test]
    fn line_diff_over_budget_falls_back_to_a_replaced_block() {
        assert!(line_diff(&["a\n", "b\n"], &["c\n", "d\n"], 1).is_none());
        assert_eq!(
            line_diff(&["a\n", "b\n"], &["a\n", "c\n"], 8),
            Some(vec![LineOp::Equal, LineOp::Delete, LineOp::Insert])
        );
    }

    #[test]
    fn unified_patch_marks_missing_final_newline() {
        // A file whose final line lacks a trailing newline must be flagged
        // rather than silently presented as newline-terminated.
        let patch = create_unified_patch("f.txt", "a\nb", "a\nB");
        assert!(
            patch.contains("\\ No newline at end of file\n"),
            "patch was: {patch:?}"
        );
    }

    struct Active;
    impl CancellationCheck for Active {
        fn check(&self) -> Result<(), String> {
            Ok(())
        }
    }
    struct Cancelled;
    impl CancellationCheck for Cancelled {
        fn check(&self) -> Result<(), String> {
            Err("cancelled by test".to_owned())
        }
    }

    fn make_temp_dir(prefix: &str) -> Result<PathBuf, RewriteError> {
        // pid keeps names unique across processes; the atomic counter keeps them
        // unique within a process even when two threads read the same clock tick.
        // Before the counter, concurrent fixtures under `cargo test` collided on
        // `{pid}-{nanos}` and panicked on `create_dir` (AlreadyExists) — the suite
        // only went green under `--test-threads=1`.
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("{prefix}{}-{unique}-{seq}", std::process::id()));
        fs::create_dir(&path).map_err(io_error)?;
        Ok(path)
    }

    fn fixture() -> (PathBuf, PathPolicy, ContentSecurity) {
        let root = make_temp_dir("octocode-rewrite-test-").expect("create fixture");
        fs::write(
            root.join("a.ts"),
            "const first = oldCall(1);\nconst second = oldCall(2);\n",
        )
        .expect("write fixture");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.clone()),
            ..Default::default()
        })
        .expect("policy");
        let security = ContentSecurity::new();
        (root, policy, security)
    }

    fn query(root: &Path) -> Value {
        json!({
            "path":root,"langType":"typescript","ruleKind":"pattern","goal": "test", "reasoning":"test",
            "pattern":"oldCall($A)","rewrite":"newCall($A)","pageSize":1
        })
    }

    #[test]
    fn syntax_breaking_template_is_rejected_before_any_commit() {
        let (root, policy, security) = fixture();
        let mut broken = query(&root);
        // Unbalanced replacement: splices cleanly but no longer parses.
        broken["rewrite"] = json!("newCall($A");
        let result = rewrite_row(
            broken.clone(),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        assert_eq!(result["errorCode"], "ast.rewrite.broken_syntax", "{result}");

        // The removed escape hatch is rejected rather than silently ignored.
        broken["allowSyntaxRegression"] = json!(true);
        assert!(serde_json::from_value::<RewriteRequest>(broken).is_err());
    }

    /// Each matched file is parsed once by the scan and once staged; the
    /// syntax check and the postcondition reuse those trees, and the rule is
    /// validated by compiling it rather than parsing an empty probe.
    #[test]
    fn preview_and_apply_parse_each_file_twice() {
        let (root, policy, security) = fixture();
        fs::write(root.join("b.ts"), "oldCall(3);\n").expect("second file");
        let mut preview = query(&root);
        preview["pageSize"] = json!(10);
        staged::take_parses();
        let first = rewrite_row(preview, &policy, &security, &Active, &Default::default());
        assert_eq!(first["totalMatches"], 3, "{first}");
        assert_eq!(staged::take_parses(), 4, "2 files × (scan + staged)");
        let mut apply = first["next"]["apply"]["query"].clone();
        apply["postconditions"] = json!([{"kind":"remainingMatches","equals":0}]);
        let options = AstRewriteRuntimeOptions {
            allow_apply: true,
            ..Default::default()
        };
        let applied = rewrite_row(apply, &policy, &security, &Active, &options);
        assert_eq!(applied["mode"], "apply", "{applied}");
        assert_eq!(
            staged::take_parses(),
            4,
            "postcondition reuses the staged parse"
        );
    }

    #[test]
    fn complete_preview_offers_an_executable_guarded_apply() {
        let (root, policy, security) = fixture();
        let first = rewrite_row(
            query(&root),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        // A partial preview page never offers apply.
        assert!(first["next"].get("apply").is_none(), "{}", first["next"]);
        let last = rewrite_row(
            first["next"]["nextPage"]["query"].clone(),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        let apply = last["next"]["apply"]["query"].clone();
        assert_eq!(apply["apply"], true, "{apply}");
        assert_eq!(apply["snapshot"], last["snapshot"]);
        let options = AstRewriteRuntimeOptions {
            allow_apply: true,
            ..Default::default()
        };
        let applied = rewrite_row(apply.clone(), &policy, &security, &Active, &options);
        assert_eq!(applied["mode"], "apply", "{applied}");
        let replay = rewrite_row(apply, &policy, &security, &Active, &options);
        assert_eq!(
            replay["errorCode"], "ast.rewrite.snapshot_changed",
            "{replay}"
        );
    }

    #[test]
    fn preview_continuation_is_lossless_and_apply_is_hash_guarded() {
        let (root, policy, security) = fixture();
        let first = rewrite_row(
            query(&root),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        assert_eq!(first["mode"], "preview");
        assert_eq!(first["totalMatches"], 2);
        assert_eq!(first["matches"].as_array().map(Vec::len), Some(1));
        let second = rewrite_row(
            first["next"]["nextPage"]["query"].clone(),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        let ids = [
            first["matches"][0]["id"].clone(),
            second["matches"][0]["id"].clone(),
        ];
        assert_ne!(ids[0], ids[1]);
        let mut apply = query(&root);
        apply["apply"] = json!(true);
        apply["snapshot"] = first["snapshot"].clone();
        // Preview reports boundary-relative `path` values; apply must accept
        // them back verbatim (absolute keys work too).
        apply["expectedHashes"] = json!({
            first["files"][0]["path"].as_str().expect("path"):
                first["files"][0]["beforeHash"].clone()
        });
        let applied = rewrite_row(
            apply,
            &policy,
            &security,
            &Active,
            &AstRewriteRuntimeOptions {
                allow_apply: true,
                ..Default::default()
            },
        );
        assert_eq!(applied["transaction"]["committed"], true);
        assert_eq!(
            fs::read_to_string(root.join("a.ts")).expect("read"),
            "const first = newCall(1);\nconst second = newCall(2);\n"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn stale_source_postcondition_and_cancellation_never_mutate() {
        let (root, policy, security) = fixture();
        let preview = rewrite_row(
            query(&root),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        let original = fs::read_to_string(root.join("a.ts")).expect("read");
        fs::write(root.join("a.ts"), format!("{original}// drift\n")).expect("drift");
        let mut apply = query(&root);
        apply["apply"] = json!(true);
        apply["snapshot"] = preview["snapshot"].clone();
        let absolute = root.join(preview["files"][0]["path"].as_str().expect("path"));
        apply["expectedHashes"] = json!({
            absolute.to_string_lossy():
                preview["files"][0]["beforeHash"].clone()
        });
        assert_eq!(
            rewrite_row(
                apply,
                &policy,
                &security,
                &Active,
                &AstRewriteRuntimeOptions {
                    allow_apply: true,
                    ..Default::default()
                }
            )["errorCode"],
            "ast.rewrite.snapshot_changed"
        );
        assert_eq!(
            rewrite_row(
                query(&root),
                &policy,
                &security,
                &Cancelled,
                &Default::default()
            )["errorCode"],
            "ast.rewrite.cancelled"
        );
        assert!(
            fs::read_to_string(root.join("a.ts"))
                .expect("read")
                .ends_with("// drift\n")
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn selection_and_failed_postcondition_preserve_unselected_bytes() {
        let (root, policy, security) = fixture();
        let mut preview_query = query(&root);
        preview_query["pageSize"] = json!(100);
        let preview = rewrite_row(
            preview_query.clone(),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        let mut apply = preview_query;
        apply["apply"] = json!(true);
        apply["snapshot"] = preview["snapshot"].clone();
        // Preview rows carry a 16-hex id prefix; apply accepts it.
        apply["selectedMatchIds"] = json!([preview["matches"][0]["id"]]);
        let absolute = root.join(preview["files"][0]["path"].as_str().expect("path"));
        apply["expectedHashes"] = json!({
            absolute.to_string_lossy():
                preview["files"][0]["beforeHash"].clone()
        });
        apply["postconditions"] = json!([{"kind":"remainingMatches","equals":0}]);
        let failed = rewrite_row(
            apply,
            &policy,
            &security,
            &Active,
            &AstRewriteRuntimeOptions {
                allow_apply: true,
                ..Default::default()
            },
        );
        assert_eq!(failed["errorCode"], "ast.rewrite.postcondition_failed");
        assert_eq!(failed["details"]["scope"], "rewrittenFiles", "{failed}");
        assert_eq!(
            failed["details"]["scannedFiles"],
            json!(["a.ts"]),
            "{failed}"
        );
        assert!(
            failed["hints"][0]
                .as_str()
                .is_some_and(|hint| hint.contains("remainingMatches")),
            "{failed}"
        );
        assert_eq!(
            fs::read_to_string(root.join("a.ts")).expect("read"),
            "const first = oldCall(1);\nconst second = oldCall(2);\n"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn preview_pages_carry_only_their_files_with_one_based_lines() {
        let (root, policy, security) = fixture();
        fs::write(root.join("b.ts"), "const third = oldCall(3);\n").expect("write b");
        let mut preview_query = query(&root);
        preview_query["pageSize"] = json!(2);
        let first = rewrite_row(
            preview_query.clone(),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        assert_eq!(first["affectedFiles"], 2, "{first}");
        let first_files = first["files"].as_array().expect("files");
        assert_eq!(first_files.len(), 1, "page 1 touches only a.ts: {first}");
        assert_eq!(first_files[0]["path"], "a.ts");
        // Match rows locate a hunk: a 16-hex id prefix, path and line; the
        // patch already shows the text and its replacement.
        let matched = &first["matches"][0];
        assert_eq!(matched["line"], 1, "{matched}");
        assert_eq!(first["matches"][1]["line"], 2);
        assert_eq!(matched["id"].as_str().map(str::len), Some(16), "{matched}");
        for dropped in ["range", "byteRange", "text", "replacement", "captures"] {
            assert!(matched.get(dropped).is_none(), "{dropped}: {matched}");
        }
        for dropped in ["afterHash", "patchBytes", "absolutePath"] {
            assert!(first_files[0].get(dropped).is_none(), "{dropped}: {first}");
        }
        // The executable/isolation receipts are debug-only diagnostics.
        assert!(first.get("executable").is_none(), "{first}");
        assert!(first.get("isolation").is_none(), "{first}");

        let second = rewrite_row(
            first["next"]["nextPage"]["query"].clone(),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        let second_files = second["files"].as_array().expect("files");
        assert_eq!(second_files.len(), 1, "{second}");
        assert_eq!(second_files[0]["path"], "b.ts");
        // The final page's guarded apply still covers every affected file.
        let apply = second["next"]["apply"]["query"].clone();
        let hashes = apply["expectedHashes"].as_object().expect("hashes");
        assert_eq!(hashes.len(), 2, "{apply}");

        let options = AstRewriteRuntimeOptions {
            allow_apply: true,
            ..Default::default()
        };
        let mut wrong = apply.clone();
        wrong["expectedHashes"]["a.ts"] = json!("0".repeat(64));
        let mismatch = rewrite_row(wrong, &policy, &security, &Active, &options);
        assert_eq!(
            mismatch["errorCode"], "ast.rewrite.hash_mismatch",
            "{mismatch}"
        );
        assert!(
            mismatch["next"]["restart"]["query"].is_object(),
            "{mismatch}"
        );
        assert!(
            mismatch["hints"][0]
                .as_str()
                .is_some_and(|hint| hint.contains("next.restart")),
            "{mismatch}"
        );

        let applied = rewrite_row(apply, &policy, &security, &Active, &options);
        assert_eq!(applied["transaction"]["committed"], true, "{applied}");
        assert!(
            applied["transaction"].get("beforeHashes").is_none(),
            "{applied}"
        );
        assert!(
            applied["transaction"].get("afterHashes").is_none(),
            "{applied}"
        );
        assert_eq!(
            applied["files"].as_array().map(Vec::len),
            Some(2),
            "{applied}"
        );
        assert_eq!(
            fs::read_to_string(root.join("b.ts")).expect("read"),
            "const third = newCall(3);\n"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn preview_pages_carry_only_the_patch_hunks_of_their_own_matches() {
        let (root, policy, security) = fixture();
        // pageSize 1 over a.ts's two matches: each page shows one match, so
        // each page's patch holds only that match's hunk, not the whole file.
        let first = rewrite_row(
            query(&root),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        let first_file = &first["files"][0];
        assert_eq!(first_file["path"], "a.ts", "{first}");
        let first_patch = first_file["patch"].as_str().expect("page-1 patch");
        assert!(
            first_patch.contains("+const first = newCall(1);"),
            "{first}"
        );
        assert!(!first_patch.contains("newCall(2)"), "{first}");
        assert_eq!(first_file["matchCount"], 2, "{first}");
        assert_eq!(first_file["patchMatchCount"], 1, "{first}");
        assert!(first_file.get("patchBytes").is_none(), "{first}");

        let second = rewrite_row(
            first["next"]["nextPage"]["query"].clone(),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        let second_file = &second["files"][0];
        assert_eq!(second_file["path"], "a.ts", "{second}");
        let second_patch = second_file["patch"].as_str().expect("page-2 patch");
        assert!(
            second_patch.contains("+const second = newCall(2);"),
            "{second}"
        );
        assert!(!second_patch.contains("newCall(1)"), "{second}");
        assert!(second_file.get("patchOnPage").is_none(), "{second}");
        assert_eq!(second_file["beforeHash"], first_file["beforeHash"]);
        assert!(second_file.get("afterHash").is_none(), "{second}");
        // The guarded apply still covers the whole file.
        let apply = &second["next"]["apply"]["query"];
        assert_eq!(apply["expectedHashes"]["a.ts"], first_file["beforeHash"]);

        // One page covering every match keeps the whole-file patch.
        let mut whole = query(&root);
        whole["pageSize"] = json!(10);
        let all = rewrite_row(whole, &policy, &security, &Active, &Default::default());
        let all_patch = all["files"][0]["patch"].as_str().expect("whole patch");
        assert!(all_patch.contains("newCall(1)") && all_patch.contains("newCall(2)"));
        assert!(all["files"][0].get("patchMatchCount").is_none(), "{all}");
        assert_eq!(
            fs::read_to_string(root.join("a.ts")).expect("read"),
            "const first = oldCall(1);\nconst second = oldCall(2);\n",
            "preview must never write"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn debug_preview_keeps_full_match_and_file_rows() {
        let (root, policy, security) = fixture();
        let mut preview_query = query(&root);
        preview_query["debug"] = json!(true);
        let preview = rewrite_row(
            preview_query,
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        let matched = &preview["matches"][0];
        assert_eq!(matched["id"].as_str().map(str::len), Some(64), "{matched}");
        assert_eq!(matched["range"]["start"]["line"], 1, "{matched}");
        assert_eq!(matched["text"], "oldCall(1)", "{matched}");
        assert_eq!(matched["replacement"], "newCall(1)", "{matched}");
        let file = &preview["files"][0];
        for kept in ["afterHash", "patchBytes", "absolutePath"] {
            assert!(file.get(kept).is_some(), "{kept}: {preview}");
        }
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn selected_match_id_prefixes_must_name_exactly_one_match() {
        let (root, policy, security) = fixture();
        let mut preview_query = query(&root);
        preview_query["pageSize"] = json!(100);
        let preview = rewrite_row(
            preview_query.clone(),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        let options = AstRewriteRuntimeOptions {
            allow_apply: true,
            ..Default::default()
        };
        let mut apply = preview["next"]["apply"]["query"].clone();
        let short = preview["matches"][1]["id"].as_str().expect("id")[..12].to_owned();
        apply["selectedMatchIds"] = json!(["f".repeat(16)]);
        let unknown = rewrite_row(apply.clone(), &policy, &security, &Active, &options);
        assert_eq!(
            unknown["errorCode"], "ast.rewrite.selection_invalid",
            "{unknown}"
        );
        apply["selectedMatchIds"] = json!([short]);
        let applied = rewrite_row(apply, &policy, &security, &Active, &options);
        assert_eq!(applied["transaction"]["committed"], true, "{applied}");
        assert_eq!(
            fs::read_to_string(root.join("a.ts")).expect("read"),
            "const first = oldCall(1);\nconst second = newCall(2);\n"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn yaml_rule_and_inferred_language_preview_like_the_explicit_object_rule() {
        let (root, policy, security) = fixture();
        let explicit = rewrite_row(
            json!({"path":root,"langType":"typescript","ruleKind":"rule","goal":"test","reasoning":"test",
                "rule":{"pattern":"oldCall($A)"},"fix":"newCall($A)","pageSize":10}),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        for row in [
            json!({"path":root,"goal":"test","reasoning":"test","rule":"pattern: oldCall($A)","fix":"newCall($A)","pageSize":10}),
            json!({"path":root,"goal":"test","reasoning":"test",
                "rule":"id: rename\nlanguage: typescript\nrule:\n  pattern: oldCall($A)\n","fix":"newCall($A)","pageSize":10}),
        ] {
            let inferred = rewrite_row(row, &policy, &security, &Active, &Default::default());
            assert_eq!(inferred["files"], explicit["files"], "{inferred}");
            assert_eq!(inferred["snapshot"], explicit["snapshot"], "{inferred}");
            assert_eq!(
                inferred["next"]["apply"]["query"]["langType"], "typescript",
                "{inferred}"
            );
            assert_eq!(
                inferred["next"]["apply"]["query"]["ruleKind"], "rule",
                "{inferred}"
            );
        }
        let foreign = serde_json::from_value::<RewriteRequest>(
            json!({"path":root,"goal":"test","reasoning":"test","rule":"rule:\n  pattern: oldCall($A)\nfix: x\n","fix":"newCall($A)"}),
        )
        .expect_err("a rule-file fix inside the rule string is not read");
        assert!(
            foreign.to_string().contains("rule file key `fix`"),
            "{foreign}"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn directory_with_several_grammars_requires_lang_type() {
        let (root, policy, security) = fixture();
        fs::write(root.join("b.rs"), "fn b() { oldCall(3); }\n").expect("rust file");
        let mixed = rewrite_row(
            json!({"path":root,"goal":"test","reasoning":"test","pattern":"oldCall($A)","rewrite":"newCall($A)"}),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        assert_eq!(
            mixed["errorCode"], "ast.rewrite.language_required",
            "{mixed}"
        );
        assert!(mixed.to_string().contains("typescript"), "{mixed}");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn interrupted_multi_file_transaction_is_rolled_back_from_journal() {
        let root = make_temp_dir("octocode-rewrite-recovery-test-").expect("fixture");
        let id = "recovery-fixture";
        let targets = [root.join("a.ts"), root.join("b.ts")];
        let before = [b"old-a\n".to_vec(), b"old-b\n".to_vec()];
        let after = [b"new-a\n".to_vec(), b"new-b\n".to_vec()];
        for (target, bytes) in targets.iter().zip(&before) {
            fs::write(target, bytes).expect("write original");
        }
        let journal_dir = journal_directory(&root);
        fs::create_dir_all(&journal_dir).expect("journal directory");
        let journal_path = journal_dir.join(format!("{JOURNAL_PREFIX}{id}.json"));
        let mut journal = Journal {
            version: 1,
            id: id.to_owned(),
            root: root.clone(),
            phase: "committing".to_owned(),
            files: targets
                .iter()
                .enumerate()
                .map(|(index, target)| JournalFile {
                    target: target.clone(),
                    stage: target.with_file_name(format!(".octocode-{id}.stage-{index}")),
                    backup: target.with_file_name(format!(".octocode-{id}.backup-{index}")),
                    before_hash: sha256(&before[index]),
                    after_hash: sha256(&after[index]),
                    state: "planned".to_owned(),
                })
                .collect(),
        };
        persist_journal(&journal_path, &journal).expect("journal");
        for (index, (file, after_bytes)) in journal
            .files
            .iter_mut()
            .zip(after.iter())
            .take(2)
            .enumerate()
        {
            fs::write(&file.stage, after_bytes).expect("stage");
            fs::rename(&file.target, &file.backup).expect("backup");
            file.state = "backed-up".to_owned();
            if index == 0 {
                fs::rename(&file.stage, &file.target).expect("promote");
                file.state = "promoted".to_owned();
            }
        }
        persist_journal(&journal_path, &journal).expect("persist interruption");

        recover_transactions(&root, &Active).expect("recover");
        for index in 0..2 {
            assert_eq!(fs::read(&targets[index]).expect("restored"), before[index]);
            assert!(!journal.files[index].stage.exists());
            assert!(!journal.files[index].backup.exists());
        }
        assert!(!journal_path.exists());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn invalid_recovery_journal_cannot_touch_an_outside_file() {
        let root = make_temp_dir("octocode-rewrite-journal-test-").expect("fixture");
        let outside = root.parent().expect("parent").join(format!(
            "octocode-rewrite-outside-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::write(&outside, b"keep\n").expect("outside");
        let id = "malicious";
        let journal = Journal {
            version: 1,
            id: id.to_owned(),
            root: root.clone(),
            phase: "committing".to_owned(),
            files: vec![JournalFile {
                target: outside.clone(),
                stage: outside.with_file_name(format!(".octocode-{id}.stage-0")),
                backup: outside.with_file_name(format!(".octocode-{id}.backup-0")),
                before_hash: sha256(b"keep\n"),
                after_hash: sha256(b"changed\n"),
                state: "planned".to_owned(),
            }],
        };
        let journal_dir = journal_directory(&root);
        fs::create_dir_all(&journal_dir).expect("journal directory");
        let path = journal_dir.join(format!("{JOURNAL_PREFIX}{id}.json"));
        persist_journal(&path, &journal).expect("journal");
        let error = recover_transactions(&root, &Active).expect_err("reject journal");
        assert_eq!(error.code, "ast.rewrite.recovery_failed");
        assert_eq!(fs::read(&outside).expect("outside preserved"), b"keep\n");
        fs::remove_file(outside).expect("outside cleanup");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn embedded_engine_supports_inline_rules_without_an_executable() {
        let (root, policy, security) = fixture();
        let result = rewrite_row(
            json!({
                "path":root,
                "langType":"typescript",
                "ruleKind":"rule",
                "goal": "test", "reasoning":"test",
                "rule":{"pattern":"oldCall($A)"},
                "fix":"newCall($A)",
                "pageSize":100,
                "debug":true
            }),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        assert_eq!(result["totalMatches"], 2);
        assert_eq!(result["isolation"]["workingDirectory"], "ephemeral");
        assert_eq!(result["executable"]["path"], "native");
        assert_eq!(result["executable"]["version"], "embedded");
        assert_eq!(result["executable"]["capabilityDigest"], "native");
        fs::remove_dir_all(root).expect("cleanup");
    }
}
