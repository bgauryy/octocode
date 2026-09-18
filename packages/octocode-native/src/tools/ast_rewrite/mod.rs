use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod journal;
mod lock;
mod output;

use journal::{commit_transaction, recover_transactions};
use lock::RootLock;
use output::{
    continuation_query, executable_value, isolation_receipt, portable_relative, success_value,
};

const DEFAULT_MAX_FILES: usize = 2_000;
const DEFAULT_MAX_MATCHES: usize = 10_000;
const DEFAULT_PAGE_SIZE: usize = 100;
const DEFAULT_MAX_PATCH_BYTES: usize = 512 * 1024;
const MAX_FILE_BYTES: usize = 1_000_000;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024;
const JOURNAL_PREFIX: &str = ".octocode-ast-rewrite-journal-";
static APPLY_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug)]
pub struct AstRewriteRuntimeOptions {
    pub allow_apply: bool,
    pub executable: Option<PathBuf>,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
    pub max_patch_bytes: usize,
}

impl Default for AstRewriteRuntimeOptions {
    fn default() -> Self {
        Self {
            allow_apply: false,
            executable: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            max_patch_bytes: DEFAULT_MAX_PATCH_BYTES,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawPosition {
    line: u32,
    column: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawByteRange {
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawRange {
    byte_offset: RawByteRange,
    start: RawPosition,
    end: RawPosition,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RawCapture {
    text: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct RawMetaVariables {
    #[serde(default)]
    single: BTreeMap<String, RawCapture>,
    #[serde(default)]
    multi: BTreeMap<String, Vec<RawCapture>>,
    #[serde(default)]
    transformed: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawMatch {
    file: String,
    text: String,
    replacement: String,
    range: RawRange,
    replacement_offsets: Option<RawByteRange>,
    #[serde(default)]
    meta_variables: RawMetaVariables,
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
    executable: &'a ExecutableReceipt,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemainingMatches {
    pub kind: String,
    pub equals: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AstRewriteQuery {
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub reasoning: Option<String>,
    pub path: String,
    pub lang_type: String,
    #[serde(default)]
    pub rule_kind: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub rewrite: Option<String>,
    #[serde(default)]
    pub rule: Option<Value>,
    #[serde(default)]
    pub constraints: Option<Value>,
    #[serde(default)]
    pub utils: Option<Value>,
    #[serde(default)]
    pub transform: Option<Value>,
    #[serde(default)]
    pub fix: Option<Value>,
    #[serde(default)]
    pub rewriters: Option<Value>,
    #[serde(default)]
    pub include: Option<Vec<String>>,
    #[serde(default)]
    pub exclude: Option<Vec<String>>,
    #[serde(default)]
    pub apply: bool,
    #[serde(default)]
    pub expected_hashes: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub selected_match_ids: Option<Vec<String>>,
    #[serde(default)]
    pub postconditions: Option<Vec<RemainingMatches>>,
    #[serde(default = "default_max_files")]
    pub max_files: usize,
    #[serde(default = "default_max_matches")]
    pub max_matches: usize,
    #[serde(default = "one")]
    pub page: usize,
    #[serde(default = "default_page_size")]
    pub page_size: usize,
    #[serde(default)]
    pub snapshot: Option<String>,
}

const fn default_max_files() -> usize {
    DEFAULT_MAX_FILES
}
const fn default_max_matches() -> usize {
    DEFAULT_MAX_MATCHES
}
const fn default_page_size() -> usize {
    DEFAULT_PAGE_SIZE
}
const fn one() -> usize {
    1
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
    fn restart(mut self, query: &AstRewriteQuery) -> Self {
        let mut restart = continuation_query(query, Path::new(&query.path));
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
            "tool":"astRewrite","query":restart,"confidence":"exact"
        }})));
        self
    }
    fn value(self) -> Value {
        let mut value = json!({
            "operation":"rewrite",
            "errorCode":self.code,
            "error":self.message
        });
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

/// Execute one canonical structural-rewrite query. Domain failures are returned
/// as typed result values so bulk callers retain per-query diagnostics.
pub fn execute_ast_rewrite_with_options(
    query: Value,
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
    query_value: Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancellation: &dyn CancellationCheck,
    options: &AstRewriteRuntimeOptions,
) -> Result<Value, RewriteError> {
    cancellation.check().map_err(cancelled)?;
    let checked = security.validate_input_parameters(&query_value);
    if !checked.is_valid {
        return Err(RewriteError::new(
            "ast.rewrite.security_validation_failed",
            checked.warnings.join("; "),
        ));
    }
    let query: AstRewriteQuery = serde_json::from_value(query_value)
        .map_err(|error| RewriteError::new("ast.rewrite.input_invalid", error.to_string()))?;
    validate_query(&query)?;
    if query.apply && !options.allow_apply {
        return Err(RewriteError::new(
            "ast.rewrite.apply_disabled",
            "Applying rewrites requires the separate astRewrite apply capability.",
        ));
    }
    if query.apply && query.snapshot.is_none() {
        return Err(RewriteError::new(
            "ast.rewrite.snapshot_required",
            "Apply requires the exact snapshot returned by preview.",
        ));
    }
    let validated = paths
        .validate(&query.path)
        .map_err(|error| RewriteError::new("ast.rewrite.root_unavailable", error.message))?;
    let root = validated.canonical;
    let metadata = fs::metadata(&root).map_err(io_error)?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(RewriteError::new(
            "ast.rewrite.root_invalid",
            "The requested rewrite path must be a file or directory.",
        ));
    }
    let boundary = if metadata.is_dir() {
        root.clone()
    } else {
        root.parent().unwrap_or(&root).to_path_buf()
    };
    let _process_guard = APPLY_LOCK.lock().map_err(|_| {
        RewriteError::new("ast.rewrite.lock_unavailable", "Rewrite lock is poisoned.")
    })?;
    let lock = RootLock::acquire(&boundary)?;
    recover_transactions(&boundary, cancellation)?;
    let executable = attest_executable(options, cancellation)?;
    if query
        .rule_kind
        .as_deref()
        .is_some_and(|kind| kind != "pattern")
        && !executable
            .capabilities
            .iter()
            .any(|capability| capability == "inline-rules")
    {
        return Err(RewriteError::new(
            "ast.rewrite.capability_incompatible",
            "This ast-grep executable does not support isolated inline rules.",
        ));
    }
    let prepared = prepare(
        &query,
        &root,
        &PrepareContext {
            boundary: &boundary,
            paths,
            security,
            cancellation,
            options,
            executable: &executable,
        },
    )?;
    let snapshot = snapshot(&query, &root, &prepared, &executable);
    if (query.apply || query.page > 1) && query.snapshot.as_deref() != Some(&snapshot) {
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
        return Ok(json!({
            "status":"empty","operation":"rewrite",
            "mode":if query.apply {"apply"} else {"preview"},
            "root":root,"executable":executable_value(&executable),"isolation":isolation_receipt(),
            "totalMatches":0,"affectedFiles":0,"matches":[],"files":[],
            "complete":true,"isPartial":false
        }));
    }
    let all_matches = prepared
        .iter()
        .flat_map(|file| file.matches.iter().cloned())
        .collect::<Vec<_>>();
    let (result_files, result_matches) = if query.apply {
        select(
            &prepared,
            &all_matches,
            query.selected_match_ids.as_deref(),
            options.max_patch_bytes,
        )?
    } else {
        (prepared.clone(), all_matches)
    };
    let transaction = if query.apply {
        validate_expected_hashes(&query, &result_files, &boundary, paths)?;
        validate_postconditions(&query, &result_files, cancellation, options, &executable)?;
        Some(commit_transaction(&boundary, &result_files, cancellation)?)
    } else {
        None
    };
    drop(lock);
    Ok(success_value(
        &query,
        &root,
        &snapshot,
        &result_files,
        &result_matches,
        &executable,
        transaction,
    ))
}

fn validate_query(query: &AstRewriteQuery) -> Result<(), RewriteError> {
    if query.path.trim().is_empty() || query.lang_type.trim().is_empty() {
        return Err(RewriteError::new(
            "ast.rewrite.input_invalid",
            "path and langType must not be blank.",
        ));
    }
    if query.page == 0 || query.page_size == 0 || query.page_size > 1_000 {
        return Err(RewriteError::new(
            "ast.rewrite.pagination_invalid",
            "page and pageSize must be positive integers and pageSize must not exceed 1000.",
        ));
    }
    if query.max_files == 0 || query.max_files > 50_000 {
        return Err(RewriteError::new(
            "ast.rewrite.input_invalid",
            "maxFiles must be between 1 and 50000.",
        ));
    }
    if query.max_matches == 0 || query.max_matches > 100_000 {
        return Err(RewriteError::new(
            "ast.rewrite.input_invalid",
            "maxMatches must be between 1 and 100000.",
        ));
    }
    match query.rule_kind.as_deref().unwrap_or("pattern") {
        "pattern"
            if query
                .pattern
                .as_deref()
                .is_some_and(|v| !v.trim().is_empty())
                && query.rewrite.is_some() => {}
        "rule" if query.rule.is_some() && query.fix.is_some() => {}
        "experimental"
            if query.rule.is_some()
                && query.fix.is_some()
                && query.transform.is_some()
                && query.rewriters.is_some() => {}
        _ => {
            return Err(RewriteError::new(
                "ast.rewrite.input_invalid",
                "The selected ruleKind is missing its required rewrite fields.",
            ));
        }
    }
    if query.apply {
        let hashes = query.expected_hashes.as_ref().ok_or_else(|| {
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

#[derive(Debug)]
struct ProcessOutput {
    success: bool,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn discover_executable(explicit: Option<&Path>) -> Option<PathBuf> {
    let validate = |candidate: &Path| {
        fs::canonicalize(candidate)
            .ok()
            .filter(|resolved| fs::metadata(resolved).is_ok_and(|metadata| metadata.is_file()))
    };
    if let Some(explicit) = explicit {
        return validate(explicit);
    }
    let mut directories = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    for common in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        let common = PathBuf::from(common);
        if !directories.contains(&common) {
            directories.push(common);
        }
    }
    let names: &[&str] = if cfg!(windows) {
        &["ast-grep.exe", "sg.exe", "ast-grep.cmd", "sg.cmd"]
    } else {
        &["ast-grep", "sg"]
    };
    directories
        .iter()
        .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
        .find_map(|candidate| validate(&candidate))
}

fn attest_executable(
    options: &AstRewriteRuntimeOptions,
    cancellation: &dyn CancellationCheck,
) -> Result<ExecutableReceipt, RewriteError> {
    let path = discover_executable(options.executable.as_deref()).ok_or_else(|| {
        RewriteError::new(
            "ast.rewrite.executable_unavailable",
            "No executable ast-grep binary was found. Install ast-grep or configure an explicit executable.",
        )
    })?;
    let version_output = run_process(
        &path,
        &["--version".to_owned()],
        None,
        options.timeout_ms,
        64 * 1024,
        cancellation,
    )?;
    let version_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&version_output.stdout),
        String::from_utf8_lossy(&version_output.stderr)
    );
    let version_re = regex::Regex::new(r"(?:^|\s)v?(\d+)\.(\d+)\.(\d+)(?:[-+\s]|$)")
        .map_err(|error| RewriteError::new("ast.rewrite.version_unreadable", error.to_string()))?;
    let parsed = version_re.captures(&version_text).and_then(|captures| {
        Some((
            format!(
                "{}.{}.{}",
                captures.get(1)?.as_str(),
                captures.get(2)?.as_str(),
                captures.get(3)?.as_str()
            ),
            [
                captures.get(1)?.as_str().parse::<u64>().ok()?,
                captures.get(2)?.as_str().parse::<u64>().ok()?,
                captures.get(3)?.as_str().parse::<u64>().ok()?,
            ],
        ))
    });
    let Some((version, tuple)) = parsed.filter(|_| version_output.success) else {
        return Err(RewriteError::new(
            "ast.rewrite.version_unreadable",
            "The discovered ast-grep executable did not report a valid version.",
        ));
    };
    if tuple < [0, 40, 0] || tuple[0] != 0 || tuple[1] > 45 {
        return Err(RewriteError::new(
            "ast.rewrite.version_incompatible",
            format!("ast-grep {version} is outside the tested 0.40.x–0.45.x compatibility window."),
        ));
    }
    let help = run_process(
        &path,
        &["run".to_owned(), "--help".to_owned()],
        None,
        options.timeout_ms,
        256 * 1024,
        cancellation,
    )?;
    if !help.success {
        return Err(RewriteError::new(
            "ast.rewrite.capability_unreadable",
            "The discovered ast-grep executable did not expose run capabilities.",
        ));
    }
    let help_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&help.stdout),
        String::from_utf8_lossy(&help.stderr)
    );
    let required = [
        "color", "globs", "json", "lang", "pattern", "rewrite", "threads",
    ];
    let mut capabilities = required
        .iter()
        .filter(|capability| help_text.contains(&format!("--{capability}")))
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    let missing = required
        .iter()
        .filter(|capability| !capabilities.iter().any(|value| value == **capability))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(RewriteError::new(
            "ast.rewrite.capability_incompatible",
            format!(
                "ast-grep {version} is missing required run capabilities: {}.",
                missing.join(", ")
            ),
        ));
    }
    let scan_help = run_process(
        &path,
        &["scan".to_owned(), "--help".to_owned()],
        None,
        options.timeout_ms,
        256 * 1024,
        cancellation,
    )?;
    if scan_help.success
        && format!(
            "{}\n{}",
            String::from_utf8_lossy(&scan_help.stdout),
            String::from_utf8_lossy(&scan_help.stderr)
        )
        .contains("--inline-rules")
    {
        capabilities.push("inline-rules".to_owned());
    }
    let executable_bytes = fs::read(&path).map_err(|_| {
        RewriteError::new(
            "ast.rewrite.executable_unreadable",
            "The discovered ast-grep executable could not be attested.",
        )
    })?;
    let executable_sha256 = sha256(executable_bytes);
    let capability_digest = sha256(
        serde_json::to_vec(&json!({
            "contract":1,
            "version":version,
            "executableSha256":executable_sha256,
            "capabilities":capabilities
        }))
        .unwrap_or_default(),
    );
    Ok(ExecutableReceipt {
        path,
        version,
        sha256: executable_sha256,
        capability_digest,
        capabilities,
    })
}

fn scan_args(query: &AstRewriteQuery, target: &Path) -> Result<Vec<String>, RewriteError> {
    let mut args = if matches!(query.rule_kind.as_deref(), Some("rule" | "experimental")) {
        vec![
            "scan".to_owned(),
            "--inline-rules".to_owned(),
            serde_json::to_string(&rule_config(query)).map_err(|error| {
                RewriteError::new("ast.rewrite.input_invalid", error.to_string())
            })?,
        ]
    } else {
        vec![
            "run".to_owned(),
            "--pattern".to_owned(),
            query.pattern.clone().unwrap_or_default(),
            "--rewrite".to_owned(),
            query.rewrite.clone().unwrap_or_default(),
            "--lang".to_owned(),
            query.lang_type.clone(),
        ]
    };
    args.extend([
        "--json=compact".to_owned(),
        "--color".to_owned(),
        "never".to_owned(),
        "--threads".to_owned(),
        "1".to_owned(),
    ]);
    for glob in query.include.as_ref().into_iter().flatten() {
        args.extend(["--globs".to_owned(), glob.clone()]);
    }
    for glob in query.exclude.as_ref().into_iter().flatten() {
        args.extend([
            "--globs".to_owned(),
            if glob.starts_with('!') {
                glob.clone()
            } else {
                format!("!{glob}")
            },
        ]);
    }
    args.push(target.to_string_lossy().into_owned());
    Ok(args)
}

fn run_scan(
    query: &AstRewriteQuery,
    target: &Path,
    executable: &ExecutableReceipt,
    options: &AstRewriteRuntimeOptions,
    cancellation: &dyn CancellationCheck,
) -> Result<Vec<RawMatch>, RewriteError> {
    let isolation = make_temp_dir("octocode-ast-rewrite-run-")?;
    let output = run_process(
        &executable.path,
        &scan_args(query, target)?,
        Some(&isolation),
        options.timeout_ms,
        options.max_output_bytes,
        cancellation,
    );
    let _ = fs::remove_dir_all(&isolation);
    let output = output?;
    let decoded = serde_json::from_slice::<Vec<RawMatch>>(&output.stdout).ok();
    let accepted_empty = output.exit_code == Some(1)
        && output.stderr.iter().all(u8::is_ascii_whitespace)
        && !output.stdout.iter().all(u8::is_ascii_whitespace)
        && decoded.as_ref().is_some_and(Vec::is_empty);
    if !output.success && !accepted_empty {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(RewriteError::new(
            "ast.rewrite.execution_failed",
            if message.is_empty() {
                "ast-grep failed.".to_owned()
            } else {
                message
            },
        ));
    }
    decoded.ok_or_else(|| {
        RewriteError::new(
            "ast.rewrite.output_invalid",
            "ast-grep returned output that does not match its versioned JSON contract.",
        )
    })
}

fn make_temp_dir(prefix: &str) -> Result<PathBuf, RewriteError> {
    for attempt in 0..100u32 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path =
            std::env::temp_dir().join(format!("{prefix}{}-{}-{attempt}", std::process::id(), now));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io_error(error)),
        }
    }
    Err(RewriteError::new(
        "ast.rewrite.io",
        "Could not allocate an isolated temporary directory.",
    ))
}

fn run_process(
    executable: &Path,
    args: &[String],
    cwd: Option<&Path>,
    timeout_ms: u64,
    max_output_bytes: usize,
    cancellation: &dyn CancellationCheck,
) -> Result<ProcessOutput, RewriteError> {
    cancellation.check().map_err(cancelled)?;
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("NO_COLOR", "1");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command
        .spawn()
        .map_err(|error| RewriteError::new("ast.rewrite.execution_failed", error.to_string()))?;
    let stdout = child.stdout.take().ok_or_else(|| {
        RewriteError::new(
            "ast.rewrite.execution_failed",
            "Could not capture ast-grep stdout.",
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        RewriteError::new(
            "ast.rewrite.execution_failed",
            "Could not capture ast-grep stderr.",
        )
    })?;
    let total = Arc::new(AtomicUsize::new(0));
    let exceeded = Arc::new(AtomicBool::new(false));
    let stdout_thread = drain_output(
        stdout,
        max_output_bytes,
        Arc::clone(&total),
        Arc::clone(&exceeded),
    );
    let stderr_thread = drain_output(
        stderr,
        max_output_bytes,
        Arc::clone(&total),
        Arc::clone(&exceeded),
    );
    let started = Instant::now();
    let status = loop {
        if let Err(message) = cancellation.check() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(cancelled(message));
        }
        if exceeded.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(RewriteError::new(
                "ast.rewrite.output_limit",
                "ast-grep exceeded the bounded process output limit.",
            )
            .terminal());
        }
        if started.elapsed() >= Duration::from_millis(timeout_ms) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(RewriteError::new(
                "ast.rewrite.timeout",
                "ast-grep exceeded the bounded execution timeout.",
            ));
        }
        match child.try_wait().map_err(io_error)? {
            Some(status) => break status,
            None => thread::sleep(Duration::from_millis(5)),
        }
    };
    let stdout = stdout_thread.join().map_err(|_| {
        RewriteError::new("ast.rewrite.execution_failed", "stdout reader failed.")
    })??;
    let stderr = stderr_thread.join().map_err(|_| {
        RewriteError::new("ast.rewrite.execution_failed", "stderr reader failed.")
    })??;
    Ok(ProcessOutput {
        success: status.success(),
        exit_code: status.code(),
        stdout,
        stderr,
    })
}

fn drain_output<R: Read + Send + 'static>(
    mut reader: R,
    limit: usize,
    total: Arc<AtomicUsize>,
    exceeded: Arc<AtomicBool>,
) -> thread::JoinHandle<Result<Vec<u8>, RewriteError>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let read = reader.read(&mut buffer).map_err(io_error)?;
            if read == 0 {
                break;
            }
            let previous = total.fetch_add(read, Ordering::AcqRel);
            if previous.saturating_add(read) > limit {
                exceeded.store(true, Ordering::Release);
                break;
            }
            output.extend_from_slice(&buffer[..read]);
        }
        Ok(output)
    })
}

fn prepare(
    query: &AstRewriteQuery,
    root: &Path,
    context: &PrepareContext<'_>,
) -> Result<Vec<PreparedFile>, RewriteError> {
    let raw_matches = run_scan(
        query,
        root,
        context.executable,
        context.options,
        context.cancellation,
    )?;
    if raw_matches.len() > query.max_matches {
        return Err(RewriteError::new(
            "ast.rewrite.match_limit",
            format!(
                "The rewrite found {} matches, exceeding maxMatches={}. Narrow the scope.",
                raw_matches.len(),
                query.max_matches
            ),
        )
        .detail(json!({"observed":raw_matches.len(),"maxMatches":query.max_matches}))
        .terminal());
    }
    let mut grouped = BTreeMap::<PathBuf, Vec<RawMatch>>::new();
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
                "ast-grep returned a target that could not be verified.",
            )
            .detail(json!({"path":matched.file}))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(RewriteError::new(
                "ast.rewrite.symlink_target",
                "ast-grep returned a symlink or non-file target; no changes were prepared.",
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
                "ast-grep returned a target outside the real requested root.",
            )
            .detail(json!({"path":matched.file})));
        }
        grouped.entry(target.canonical).or_default().push(matched);
    }
    if grouped.len() > query.max_files {
        return Err(RewriteError::new(
            "ast.rewrite.file_limit",
            format!(
                "The rewrite affects {} files, exceeding maxFiles={}.",
                grouped.len(),
                query.max_files
            ),
        )
        .terminal());
    }
    let mut files = Vec::new();
    let mut total_patch_bytes = 0usize;
    for (absolute, raw) in grouped {
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
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn rule_config(query: &AstRewriteQuery) -> Value {
    let mut config = Map::new();
    config.insert("id".to_owned(), json!("octocode-inline-rewrite"));
    config.insert("language".to_owned(), json!(query.lang_type));
    config.insert("severity".to_owned(), json!("warning"));
    config.insert(
        "message".to_owned(),
        json!("Octocode inline structural rewrite"),
    );
    if query.rule_kind.as_deref().unwrap_or("pattern") == "pattern" {
        config.insert("rule".to_owned(), json!({"pattern":query.pattern}));
        config.insert("fix".to_owned(), json!(query.rewrite));
    } else {
        config.insert("rule".to_owned(), query.rule.clone().unwrap_or(Value::Null));
        config.insert("fix".to_owned(), query.fix.clone().unwrap_or(Value::Null));
        for (key, value) in [
            ("constraints", query.constraints.as_ref()),
            ("utils", query.utils.as_ref()),
            ("transform", query.transform.as_ref()),
            ("rewriters", query.rewriters.as_ref()),
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
            let public = json!({
                "id":id,"path":path,
                "byteRange":{"start":start,"end":end},
                "range":matched.range,"text":matched.text,"replacement":matched.replacement,
                "captures":captures
            });
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
            return Err(RewriteError::new(
                "ast.rewrite.overlap",
                "The rewrite engine returned overlapping replacement ranges; no changes were prepared.",
            )
            .detail(json!({"path":path})));
        }
    }
    Ok(matches)
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
    files: &[PreparedFile],
    matches: &[PreparedMatch],
    selected: Option<&[String]>,
    max_patch_bytes: usize,
) -> Result<(Vec<PreparedFile>, Vec<PreparedMatch>), RewriteError> {
    let Some(selected) = selected else {
        return Ok((files.to_vec(), matches.to_vec()));
    };
    let selected = selected.iter().cloned().collect::<BTreeSet<_>>();
    let known = matches
        .iter()
        .map(|matched| matched.id.clone())
        .collect::<BTreeSet<_>>();
    let unknown = selected.difference(&known).cloned().collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(RewriteError::new(
            "ast.rewrite.selection_invalid",
            "selectedMatchIds contains IDs that are not part of this snapshot.",
        )
        .detail(json!({"unknown":unknown})));
    }
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
        selected_file.patch = patch;
        selected_file.matches = file_matches.clone();
        selected_matches.extend(file_matches);
        selected_files.push(selected_file);
    }
    Ok((selected_files, selected_matches))
}

fn validate_expected_hashes(
    query: &AstRewriteQuery,
    files: &[PreparedFile],
    boundary: &Path,
    paths: &PathPolicy,
) -> Result<(), RewriteError> {
    let mut expected = BTreeMap::new();
    for (path, hash) in query.expected_hashes.as_ref().into_iter().flatten() {
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(RewriteError::new(
                "ast.rewrite.expected_hash_invalid",
                "Expected hash values must be SHA-256 hex digests.",
            )
            .detail(json!({"path":path})));
        }
        let validated = paths.validate_read(path).map_err(|_| {
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
    query: &AstRewriteQuery,
    files: &[PreparedFile],
    cancellation: &dyn CancellationCheck,
    options: &AstRewriteRuntimeOptions,
    executable: &ExecutableReceipt,
) -> Result<(), RewriteError> {
    let Some(postconditions) = query.postconditions.as_ref() else {
        return Ok(());
    };
    if postconditions.is_empty() {
        return Ok(());
    }
    let mirror = make_temp_dir("octocode-ast-rewrite-postcondition-")?;
    let staged = (|| -> Result<(), RewriteError> {
        for file in files {
            cancellation.check().map_err(cancelled)?;
            let target = mirror.join(&file.path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(io_error)?;
            }
            fs::write(&target, &file.after).map_err(io_error)?;
            fs::set_permissions(&target, file.permissions.clone()).map_err(io_error)?;
        }
        Ok(())
    })();
    if let Err(error) = staged {
        let _ = fs::remove_dir_all(&mirror);
        return Err(error);
    }
    let scan = run_scan(query, &mirror, executable, options, cancellation);
    let _ = fs::remove_dir_all(&mirror);
    let remaining = scan
        .map_err(|_| {
            RewriteError::new(
                "ast.rewrite.postcondition_execution_failed",
                "The postcondition scan could not be completed; no files were changed.",
            )
        })?
        .len();
    for postcondition in postconditions {
        if postcondition.kind != "remainingMatches" || postcondition.equals != remaining {
            return Err(RewriteError::new(
                "ast.rewrite.postcondition_failed",
                "A staged rewrite postcondition failed; no files were changed.",
            )
            .detail(json!({
                "kind":postcondition.kind,"expected":postcondition.equals,"observed":remaining
            })));
        }
    }
    Ok(())
}

fn snapshot(
    query: &AstRewriteQuery,
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
    let rule_spec = if matches!(query.rule_kind.as_deref(), Some("rule" | "experimental")) {
        let mut spec = Map::new();
        spec.insert(
            "ruleKind".to_owned(),
            json!(query.rule_kind.as_deref().unwrap_or("rule")),
        );
        spec.insert("rule".to_owned(), query.rule.clone().unwrap_or(Value::Null));
        for (key, value) in [
            ("constraints", query.constraints.as_ref()),
            ("utils", query.utils.as_ref()),
            ("transform", query.transform.as_ref()),
            ("fix", query.fix.as_ref()),
        ] {
            if let Some(value) = value {
                spec.insert(key.to_owned(), value.clone());
            }
        }
        if query.rule_kind.as_deref() == Some("experimental")
            && let Some(rewriters) = &query.rewriters
        {
            spec.insert("rewriters".to_owned(), rewriters.clone());
        }
        Value::Object(spec)
    } else {
        json!({
            "ruleKind":"pattern",
            "pattern":query.pattern,
            "rewrite":query.rewrite
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
            "langType":query.lang_type,
            "ruleSpec":rule_spec,
            "include":query.include.as_deref().unwrap_or(&[]),
            "exclude":query.exclude.as_deref().unwrap_or(&[]),
            "maxFiles":query.max_files,
            "maxMatches":query.max_matches,
            "pageSize":query.page_size,
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

fn create_unified_patch(path: &str, before: &str, after: &str) -> String {
    if before == after {
        return String::new();
    }
    let old = before.lines().collect::<Vec<_>>();
    let new = after.lines().collect::<Vec<_>>();
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
    let context_start = prefix.saturating_sub(3);
    let old_end = old
        .len()
        .min(old.len().saturating_sub(suffix).saturating_add(3));
    let leading = &old[context_start..prefix];
    let removed = &old[prefix..old.len().saturating_sub(suffix)];
    let added = &new[prefix..new.len().saturating_sub(suffix)];
    let trailing = &old[old.len().saturating_sub(suffix)..old_end];
    let mut lines = vec![
        format!("--- a/{path}"),
        format!("+++ b/{path}"),
        format!(
            "@@ -{},{} +{},{} @@",
            context_start + 1,
            leading.len() + removed.len() + trailing.len(),
            context_start + 1,
            leading.len() + added.len() + trailing.len()
        ),
    ];
    lines.extend(leading.iter().map(|line| format!(" {line}")));
    lines.extend(removed.iter().map(|line| format!("-{line}")));
    lines.extend(added.iter().map(|line| format!("+{line}")));
    lines.extend(trailing.iter().map(|line| format!(" {line}")));
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::journal::{journal_directory, persist_journal};
    use super::*;
    use crate::{policy::path::PathPolicyConfig, security::SecurityRegistry};
    use std::sync::Arc;

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
        let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
        (root, policy, security)
    }

    fn query(root: &Path) -> Value {
        json!({
            "path":root,"langType":"typescript","ruleKind":"pattern",
            "pattern":"oldCall($A)","rewrite":"newCall($A)","pageSize":1
        })
    }

    #[test]
    fn preview_continuation_is_lossless_and_apply_is_hash_guarded() {
        let (root, policy, security) = fixture();
        let first = execute_ast_rewrite_with_options(
            query(&root),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        assert_eq!(first["mode"], "preview");
        assert_eq!(first["totalMatches"], 2);
        assert_eq!(first["matches"].as_array().map(Vec::len), Some(1));
        let second = execute_ast_rewrite_with_options(
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
        apply["expectedHashes"] = json!({
            first["files"][0]["absolutePath"].as_str().expect("path"):
                first["files"][0]["beforeHash"].clone()
        });
        let applied = execute_ast_rewrite_with_options(
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
        let preview = execute_ast_rewrite_with_options(
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
        apply["expectedHashes"] = json!({
            preview["files"][0]["absolutePath"].as_str().expect("path"):
                preview["files"][0]["beforeHash"].clone()
        });
        assert_eq!(
            execute_ast_rewrite_with_options(
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
            execute_ast_rewrite_with_options(
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
        let preview = execute_ast_rewrite_with_options(
            preview_query.clone(),
            &policy,
            &security,
            &Active,
            &Default::default(),
        );
        let mut apply = preview_query;
        apply["apply"] = json!(true);
        apply["snapshot"] = preview["snapshot"].clone();
        apply["selectedMatchIds"] = json!([preview["matches"][0]["id"]]);
        apply["expectedHashes"] = json!({
            preview["files"][0]["absolutePath"].as_str().expect("path"):
                preview["files"][0]["beforeHash"].clone()
        });
        apply["postconditions"] = json!([{"kind":"remainingMatches","equals":0}]);
        let failed = execute_ast_rewrite_with_options(
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
        assert_eq!(
            fs::read_to_string(root.join("a.ts")).expect("read"),
            "const first = oldCall(1);\nconst second = oldCall(2);\n"
        );
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

    #[cfg(unix)]
    fn executable_script(root: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = root.join(name);
        fs::write(&path, body).expect("script");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("chmod");
        path
    }

    #[cfg(unix)]
    #[test]
    fn executable_hash_timeout_and_output_caps_are_enforced() {
        let (root, policy, security) = fixture();
        let forwarding = executable_script(
            &root,
            "ast-grep-forwarder",
            "#!/bin/sh\nexec /opt/homebrew/bin/ast-grep \"$@\"\n",
        );
        let options = AstRewriteRuntimeOptions {
            allow_apply: true,
            executable: Some(forwarding.clone()),
            ..Default::default()
        };
        let preview =
            execute_ast_rewrite_with_options(query(&root), &policy, &security, &Active, &options);
        assert!(preview.get("snapshot").is_some());
        fs::write(
            &forwarding,
            "#!/bin/sh\n# changed executable bytes\nexec /opt/homebrew/bin/ast-grep \"$@\"\n",
        )
        .expect("replace executable");
        let mut apply = query(&root);
        apply["apply"] = json!(true);
        apply["snapshot"] = preview["snapshot"].clone();
        apply["expectedHashes"] = json!({
            preview["files"][0]["absolutePath"].as_str().expect("path"):
                preview["files"][0]["beforeHash"].clone()
        });
        assert_eq!(
            execute_ast_rewrite_with_options(apply, &policy, &security, &Active, &options)["errorCode"],
            "ast.rewrite.snapshot_changed"
        );

        let noisy = executable_script(
            &root,
            "ast-grep-noisy",
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf 'ast-grep 0.45.0'; exit 0; fi\nif [ \"$2\" = \"--help\" ]; then printf '%s' '--color --globs --json --lang --pattern --rewrite --threads --inline-rules'; exit 0; fi\nprintf 'this-output-is-deliberately-too-large-for-the-process-output-cap'\n",
        );
        let noisy_options = AstRewriteRuntimeOptions {
            executable: Some(noisy),
            max_output_bytes: 24,
            ..Default::default()
        };
        assert_eq!(
            execute_ast_rewrite_with_options(
                query(&root),
                &policy,
                &security,
                &Active,
                &noisy_options,
            )["errorCode"],
            "ast.rewrite.output_limit"
        );

        let hanging = executable_script(
            &root,
            "ast-grep-hanging",
            "#!/bin/sh\nwhile :; do :; done\n",
        );
        let timeout_options = AstRewriteRuntimeOptions {
            executable: Some(hanging),
            timeout_ms: 20,
            ..Default::default()
        };
        assert_eq!(
            execute_ast_rewrite_with_options(
                query(&root),
                &policy,
                &security,
                &Active,
                &timeout_options,
            )["errorCode"],
            "ast.rewrite.timeout"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}
