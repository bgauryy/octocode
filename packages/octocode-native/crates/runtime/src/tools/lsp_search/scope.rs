//! The search scope of a request's lexical scans and leads.
//!
//! The language server's workspace root is the *nearest* project marker (a
//! package's `package.json`, a member crate's `Cargo.toml`): right for the
//! server, too narrow for "who else mentions this name". Lexical scans
//! (importer candidates, the Rust cfg-gate count, `textOnlyFiles`) and the
//! `textSearch` lead use one wider scope instead:
//!
//! 1. the nearest ancestor holding `.git` (a directory or a worktree file);
//! 2. else the outermost workspace manifest (`pnpm-workspace.yaml`,
//!    `lerna.json`, `package.json` with `workspaces`, `Cargo.toml` with
//!    `[workspace]`, `go.work`);
//! 3. else the language server's workspace root;
//!
//! clamped to the read policy: an unauthorized root becomes the outermost
//! authorized ancestor of the file. The scope is never the process cwd.
//! `include` is the anchor language's family globs, from the engine grammar
//! registry.

use super::blocking_cancellable;
use super::failure::LspFailure;
use super::render::{uri_to_path, word_pattern};
use crate::policy::path::PathPolicy;
use crate::tools::cancel::CancellationCheck;
use octocode_engine::portable::search_ripgrep_cancellable;
use octocode_engine::types::RipgrepSearchOptions;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// Language ids that share one lexical family (one server answers them).
const FAMILIES: [&[&str]; 2] = [
    &[
        "typescript",
        "typescriptreact",
        "javascript",
        "javascriptreact",
    ],
    &["c", "cpp", "cuda", "objective-c", "objective-cpp"],
];

/// Project files whose edits change what a language server answers.
const PROJECT_FILES: [&str; 12] = [
    "package.json",
    "tsconfig.json",
    "jsconfig.json",
    "Cargo.toml",
    "Cargo.lock",
    "go.mod",
    "go.work",
    "pyproject.toml",
    "setup.cfg",
    "compile_commands.json",
    "compile_flags.txt",
    ".clangd",
];

/// Bounds of the scope fingerprint walk; past either, responses are not
/// reused across requests (correct, only slower).
const FINGERPRINT_MAX_FILES: usize = 20_000;
const FINGERPRINT_BUDGET: Duration = Duration::from_millis(150);
/// A file modified this recently also hashes its content: some filesystems
/// keep coarse mtimes, so a same-second edit keeps its length and mtime.
const FRESH_MTIME: Duration = Duration::from_secs(2);
/// A larger unsettled source disables cache reuse rather than reading it
/// whole merely to decide whether a lexical scan may be reused.
const FRESH_CONTENT_MAX_BYTES: usize = 1024 * 1024;
/// Bound of the text scan; a slower scan reports no files (no
/// `textOnlyFiles`, and importer recovery reports a failed scan).
const TEXT_SCAN_BUDGET: Duration = Duration::from_secs(3);

/// Text scans by `(root, include, symbol, scope fingerprint, policy)`. The
/// fingerprint covers every family file under the root, so an unchanged
/// fingerprint proves the scan would find the same files; a scan without a
/// fingerprint (walk past its bounds) is never stored.
static TEXT_SCANS: LazyLock<Mutex<HashMap<String, Arc<BTreeSet<String>>>>> =
    LazyLock::new(Mutex::default);
/// Stored text scans per process; past it the store starts over.
const TEXT_SCANS_MAX: usize = 256;

/// The files a word-bounded name appears in, for one symbol.
struct TextScan {
    symbol: String,
    /// Canonical paths; `None` when the scan failed or hit a bound.
    files: Option<Arc<BTreeSet<String>>>,
}

pub(super) struct Scope {
    /// Canonical directory every lexical scan and lead covers.
    pub(super) root: String,
    /// Family globs (`*.ts`, …); empty means every file.
    pub(super) include: Vec<String>,
    text: Mutex<Option<TextScan>>,
    /// Digest of every family and project file under `root`, when the
    /// request computed one within its bounds (see `fingerprint`).
    fingerprint: Mutex<Option<String>>,
    /// Canonical files the language server's answer covers (listed, or
    /// verified as importers), recorded by the operation.
    answered: Mutex<BTreeSet<String>>,
}

impl Scope {
    pub(super) fn new(root: String, include: Vec<String>) -> Self {
        Self {
            root,
            include,
            text: Mutex::new(None),
            fingerprint: Mutex::new(None),
            answered: Mutex::new(BTreeSet::new()),
        }
    }

    /// The scope of a request anchored at `anchor` (a file or, for a
    /// workspace-root query, a directory).
    pub(super) fn resolve(
        anchor: &str,
        server_root: &str,
        policy: &PathPolicy,
        language_id: Option<&str>,
    ) -> Self {
        Self::new(
            search_root(Path::new(anchor), server_root, policy),
            family_include(language_id, Path::new(anchor)),
        )
    }

    /// Record the scope fingerprint this request computed; text scans under
    /// it are shared with later requests that compute the same one.
    pub(super) fn set_fingerprint(&self, fingerprint: String) {
        if let Ok(mut slot) = self.fingerprint.lock() {
            *slot = Some(fingerprint);
        }
    }

    /// Record files (paths or `file://` uris) the server's answer covers.
    pub(super) fn answer(&self, files: impl IntoIterator<Item = impl AsRef<str>>) {
        if let Ok(mut answered) = self.answered.lock() {
            answered.extend(
                files
                    .into_iter()
                    .map(|file| canonical(&uri_to_path(file.as_ref()))),
            );
        }
    }

    /// Files under the scope that spell `symbol` as a word (canonical
    /// paths), scanned once per request and symbol. `Ok(None)` when the
    /// scan failed or hit a bound.
    pub(super) async fn text_files(
        &self,
        symbol: &str,
        policy: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Result<Option<Arc<BTreeSet<String>>>, LspFailure> {
        if let Some(scan) = self.text.lock().ok().as_ref().and_then(|scan| {
            scan.as_ref()
                .filter(|scan| scan.symbol == symbol)
                .map(|scan| scan.files.clone())
        }) {
            return Ok(scan);
        }
        let stored = self
            .fingerprint
            .lock()
            .ok()
            .and_then(|fingerprint| fingerprint.clone())
            .map(|fingerprint| {
                crate::digest::sha256(
                    format!(
                        "{}\u{0}{}\u{0}{symbol}\u{0}{fingerprint}\u{0}{policy:?}",
                        self.root,
                        self.include.join("\u{0}")
                    )
                    .as_bytes(),
                )
            });
        let hit = stored.as_ref().and_then(|key| {
            TEXT_SCANS
                .lock()
                .ok()
                .and_then(|scans| scans.get(key).cloned())
        });
        let files = match hit {
            Some(files) => Some(files),
            None => {
                let files = scan_text(&self.root, &self.include, symbol, policy, cancel)
                    .await?
                    .map(Arc::new);
                if let (Some(key), Some(files)) = (stored, &files)
                    && let Ok(mut scans) = TEXT_SCANS.lock()
                {
                    if scans.len() >= TEXT_SCANS_MAX {
                        scans.clear();
                    }
                    scans.insert(key, Arc::clone(files));
                }
                files
            }
        };
        if let Ok(mut slot) = self.text.lock() {
            *slot = Some(TextScan {
                symbol: symbol.to_owned(),
                files: files.clone(),
            });
        }
        Ok(files)
    }

    /// Files the text scan of `symbol` found that the server's answer does
    /// not cover; `None` when either side is unknown.
    pub(super) fn text_only_files(&self, symbol: &str) -> Option<Vec<String>> {
        let text = self.text.lock().ok()?;
        let files = text
            .as_ref()
            .filter(|scan| scan.symbol == symbol)?
            .files
            .clone()?;
        let answered = self.answered.lock().ok()?;
        if answered.is_empty() {
            return None;
        }
        Some(
            files
                .iter()
                .filter(|file| !answered.contains(*file))
                .cloned()
                .collect(),
        )
    }

    /// The owned inputs of the fingerprint walk, for a blocking worker.
    pub(super) fn walk_inputs(&self) -> WalkInputs {
        WalkInputs {
            root: PathBuf::from(&self.root),
            include: self.include.clone(),
        }
    }
}

/// What the scope fingerprint walks: the scope root and its family globs.
pub(super) struct WalkInputs {
    root: PathBuf,
    include: Vec<String>,
}

impl WalkInputs {
    /// Digest of every family and project file under the scope (relative
    /// path, length, mtime; content too for a fresh mtime), so any edit,
    /// addition, or removal changes it. `None` past the walk's bounds.
    pub(super) fn fingerprint(&self, stopped: &(dyn Fn() -> bool + Sync)) -> Option<String> {
        fingerprint(&self.root, &self.include, stopped, FINGERPRINT_BUDGET)
    }
}

tokio::task_local! {
    /// Canonical forms already resolved by this request (see `canonical`).
    static CANONICAL: RefCell<HashMap<String, String>>;
}

/// Run one request with its own canonical-path memo.
pub(super) async fn with_canonical_memo<F: std::future::Future>(request: F) -> F::Output {
    CANONICAL.scope(RefCell::default(), request).await
}

/// `path` with symlinks resolved, or as given when it cannot be (gone, or
/// not local). Inside [`with_canonical_memo`] each spelling is resolved
/// once per request: a server answer names the same few files hundreds of
/// times. Outside it (blocking workers, tests) every call resolves.
pub(super) fn canonical(path: &str) -> String {
    let resolve = || {
        std::fs::canonicalize(path)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_owned())
    };
    CANONICAL
        .try_with(|memo| {
            if let Some(hit) = memo.borrow().get(path) {
                return hit.clone();
            }
            let resolved = resolve();
            memo.borrow_mut().insert(path.to_owned(), resolved.clone());
            resolved
        })
        .unwrap_or_else(|_| resolve())
}

/// The scope root: VCS root, else outermost workspace manifest, else the
/// server root; clamped to the policy, never the process cwd.
fn search_root(anchor: &Path, server_root: &str, policy: &PathPolicy) -> String {
    let start = if anchor.is_dir() {
        anchor
    } else {
        anchor.parent().unwrap_or(anchor)
    };
    let chosen = start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .or_else(|| {
            start
                .ancestors()
                .filter(|dir| is_workspace_manifest_root(dir))
                .last()
        })
        .map_or_else(|| PathBuf::from(server_root), Path::to_path_buf);
    let authorized = |dir: &Path| {
        policy
            .validate(dir)
            .ok()
            .filter(|valid| valid.canonical.is_dir())
            .map(|valid| valid.canonical.to_string_lossy().into_owned())
    };
    // The chosen root must contain the anchor and be readable; otherwise
    // the widest authorized directory on the anchor's own path.
    if start.starts_with(&chosen)
        && let Some(root) = authorized(&chosen)
    {
        return root;
    }
    start
        .ancestors()
        .map_while(authorized)
        .last()
        .unwrap_or_else(|| start.to_string_lossy().into_owned())
}

fn is_workspace_manifest_root(dir: &Path) -> bool {
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
    dir.join("pnpm-workspace.yaml").is_file()
        || dir.join("lerna.json").is_file()
        || dir.join("go.work").is_file()
        || read("package.json")
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .is_some_and(|manifest| manifest.get("workspaces").is_some())
        || read("Cargo.toml").is_some_and(|text| {
            text.lines()
                .any(|line| line.trim_start().starts_with("[workspace]"))
        })
}

/// `*.ext` globs of every grammar in `language_id`'s family; the anchor's
/// own extension for a language the registry does not know.
fn family_include(language_id: Option<&str>, anchor: &Path) -> Vec<String> {
    let family: Vec<&str> = match language_id {
        Some(id) => FAMILIES
            .iter()
            .find(|family| family.contains(&id))
            .map_or_else(|| vec![id], |family| family.to_vec()),
        None => Vec::new(),
    };
    let mut globs = octocode_engine::signatures::languages::all_entries()
        .iter()
        .filter(|entry| entry.language_id.is_some_and(|id| family.contains(&id)))
        .flat_map(|entry| entry.extensions.iter())
        .map(|extension| format!("*.{extension}"))
        .collect::<Vec<_>>();
    if globs.is_empty()
        && let Some(extension) = anchor.extension().filter(|_| !anchor.is_dir())
    {
        globs.push(format!("*.{}", extension.to_string_lossy()));
    }
    globs
}

/// Word-bounded files-only scan of `root` (family `include`, read policy),
/// observing `cancel` and the scan bounds.
async fn scan_text(
    root: &str,
    include: &[String],
    symbol: &str,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Option<BTreeSet<String>>, LspFailure> {
    if symbol.trim().is_empty() {
        return Ok(None);
    }
    let options = RipgrepSearchOptions {
        path: root.to_owned(),
        pattern: word_pattern(symbol),
        files_only: Some(true),
        include: (!include.is_empty()).then(|| include.to_vec()),
        ..RipgrepSearchOptions::default()
    };
    let filter = Arc::new(policy.clone());
    let base = root.to_owned();
    let scanned = blocking_cancellable(cancel, move |stopped| {
        let started = Instant::now();
        let bounded = || stopped() || started.elapsed() > TEXT_SCAN_BUDGET;
        search_ripgrep_cancellable(options, filter, &bounded)
            .ok()
            .filter(|_| started.elapsed() <= TEXT_SCAN_BUDGET)
            .map(|parsed| {
                parsed
                    .files
                    .into_iter()
                    .map(|file| {
                        let path = Path::new(&file.path);
                        if path.is_absolute() {
                            canonical(&file.path)
                        } else {
                            canonical(&Path::new(&base).join(path).to_string_lossy())
                        }
                    })
                    .collect::<BTreeSet<_>>()
            })
    })
    .await?
    .flatten();
    Ok(scanned)
}

fn fingerprint(
    root: &Path,
    include: &[String],
    stopped: &(dyn Fn() -> bool + Sync),
    budget: Duration,
) -> Option<String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let extensions = include
        .iter()
        .filter_map(|glob| glob.strip_prefix("*."))
        .collect::<Vec<_>>();
    let started = Instant::now();
    let now = SystemTime::now();
    let entries = Mutex::new(Vec::new());
    let abandoned = AtomicBool::new(false);
    // One digest line per family or project file: relative path, length,
    // mtime, and the content hash of a freshly modified file.
    let line = |entry: ignore::DirEntry| -> Option<String> {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            return None;
        }
        let path = entry.path();
        let name = path.file_name().map(|name| name.to_string_lossy());
        let family = path.extension().is_some_and(|extension| {
            extensions.is_empty() || extensions.contains(&extension.to_string_lossy().as_ref())
        });
        if !family && !name.is_some_and(|name| PROJECT_FILES.contains(&name.as_ref())) {
            return None;
        }
        let meta = entry.metadata().ok()?;
        let Some(stamp) = crate::tools::source::cache_stamp(&meta) else {
            abandoned.store(true, Ordering::Relaxed);
            return None;
        };
        let modified = meta.modified().ok();
        let fresh = modified
            .is_none_or(|time| !now.duration_since(time).is_ok_and(|age| age >= FRESH_MTIME));
        let content = if fresh {
            match crate::tools::source::read_bounded(path, FRESH_CONTENT_MAX_BYTES) {
                Ok(bytes) => crate::digest::sha256(&bytes),
                Err(_) => {
                    abandoned.store(true, Ordering::Relaxed);
                    return None;
                }
            }
        } else {
            String::new()
        };
        let relative = path.strip_prefix(root).unwrap_or(path);
        Some(format!(
            "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}\u{0}{content}",
            relative.to_string_lossy(),
            stamp.size,
            stamp.modified_ns,
            stamp.changed_ns,
            stamp.inode,
        ))
    };
    // A parallel walk: the digest is order-free (lines are sorted).
    crate::tools::pruned_walk(root, crate::tools::syntax_prune(&[]))
        .build_parallel()
        .run(|| {
            Box::new(|entry| {
                if abandoned.load(Ordering::Relaxed) {
                    return ignore::WalkState::Quit;
                }
                if stopped() || started.elapsed() > budget {
                    abandoned.store(true, Ordering::Relaxed);
                    return ignore::WalkState::Quit;
                }
                let Some(line) = entry.ok().and_then(&line) else {
                    return ignore::WalkState::Continue;
                };
                let Ok(mut entries) = entries.lock() else {
                    abandoned.store(true, Ordering::Relaxed);
                    return ignore::WalkState::Quit;
                };
                entries.push(line);
                if entries.len() > FINGERPRINT_MAX_FILES {
                    abandoned.store(true, Ordering::Relaxed);
                    return ignore::WalkState::Quit;
                }
                ignore::WalkState::Continue
            })
        });
    if abandoned.load(Ordering::Relaxed) {
        return None;
    }
    let mut entries = entries.into_inner().ok()?;
    entries.sort_unstable();
    Some(crate::digest::sha256(entries.join("\n").as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::test_support::workspace_policy;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().canonicalize().expect("canonical");
        (dir, root)
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, text).expect("write");
    }

    fn resolve(file: &Path, server_root: &Path, policy_root: &Path, id: &str) -> Scope {
        Scope::resolve(
            &file.to_string_lossy(),
            &server_root.to_string_lossy(),
            &workspace_policy(policy_root),
            Some(id),
        )
    }

    #[test]
    fn scope_prefers_vcs_root_over_nearest_package() {
        let (_dir, root) = fixture();
        std::fs::create_dir_all(root.join(".git")).expect("git");
        write(
            &root.join("package.json"),
            r#"{"workspaces":["packages/*"]}"#,
        );
        write(&root.join("packages/a/package.json"), "{}");
        let file = root.join("packages/a/src/x.ts");
        write(&file, "export const x = 1;\n");
        let scope = resolve(&file, &root.join("packages/a"), &root, "typescript");
        assert_eq!(scope.root, root.to_string_lossy());
        for glob in ["*.ts", "*.tsx", "*.js", "*.mts", "*.cjs"] {
            assert!(
                scope.include.iter().any(|g| g == glob),
                "{glob}: {:?}",
                scope.include
            );
        }
        assert!(!scope.include.iter().any(|g| g == "*.rs"));
    }

    #[test]
    fn scope_without_vcs_uses_outermost_cargo_workspace() {
        let (_dir, root) = fixture();
        write(
            &root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"tokio\"]\n",
        );
        write(
            &root.join("tokio/Cargo.toml"),
            "[package]\nname = \"tokio\"\n",
        );
        let file = root.join("tokio/src/lib.rs");
        write(&file, "pub fn spawn_blocking() {}\n");
        let scope = resolve(&file, &root.join("tokio"), &root, "rust");
        assert_eq!(scope.root, root.to_string_lossy());
        assert_eq!(scope.include, vec!["*.rs".to_owned()]);
    }

    #[test]
    fn scope_is_clamped_to_policy_root_and_never_cwd() {
        let (_dir, root) = fixture();
        std::fs::create_dir_all(root.join("repo/.git")).expect("git");
        let file = root.join("repo/packages/a/x.ts");
        write(&file, "x\n");
        let packages = root.join("repo/packages");
        let scope = resolve(
            &file,
            &root.join("repo/packages/a"),
            &packages,
            "typescript",
        );
        assert_eq!(scope.root, packages.to_string_lossy());
        // No marker anywhere and a server root outside the policy: the
        // widest authorized directory on the file's path, not the cwd.
        let lone = root.join("lone/deep/y.py");
        write(&lone, "y = 1\n");
        let cwd = std::env::current_dir().expect("cwd");
        let scope = resolve(&lone, &cwd, &root.join("lone"), "python");
        assert_eq!(scope.root, root.join("lone").to_string_lossy());
        assert_eq!(scope.include, vec!["*.py".to_owned(), "*.pyi".to_owned()]);
    }

    #[test]
    fn fingerprint_changes_on_edit_add_delete() {
        let (_dir, root) = fixture();
        write(&root.join("a.ts"), "export const a = 1;\n");
        write(&root.join("notes.md"), "notes\n");
        // A generous bound: the test measures what the digest covers, not
        // the walk's speed under a loaded test runner.
        let scope = |stopped: &(dyn Fn() -> bool + Sync)| {
            fingerprint(
                &root,
                &["*.ts".to_owned()],
                stopped,
                Duration::from_secs(60),
            )
        };
        let never = || false;
        let first = scope(&never).expect("fingerprint");
        assert_eq!(scope(&never).as_ref(), Some(&first));
        // An unrelated extension does not change it.
        write(&root.join("notes.md"), "other notes\n");
        assert_eq!(scope(&never).as_ref(), Some(&first));
        // A same-length edit within the mtime granularity still does.
        write(&root.join("a.ts"), "export const a = 2;\n");
        let edited = scope(&never).expect("edited");
        assert_ne!(edited, first);
        write(&root.join("b.ts"), "export const b = 1;\n");
        let added = scope(&never).expect("added");
        assert_ne!(added, edited);
        std::fs::remove_file(root.join("b.ts")).expect("remove");
        assert_eq!(scope(&never).as_ref(), Some(&edited));
        // A project file edit changes it too.
        write(&root.join("tsconfig.json"), "{}");
        assert_ne!(scope(&never).as_ref(), Some(&edited));
        // A stopped walk reports no fingerprint.
        assert_eq!(scope(&|| true), None);
    }

    #[test]
    fn fingerprint_changes_after_same_size_edit_with_restored_old_mtime() {
        let (_dir, root) = fixture();
        let path = root.join("a.ts");
        write(&path, "export const a = 1;\n");
        let old_time = std::time::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        let set_old_time = || {
            std::fs::File::open(&path)
                .expect("file")
                .set_times(std::fs::FileTimes::new().set_modified(old_time))
                .expect("mtime");
        };
        set_old_time();
        let digest = || {
            fingerprint(
                &root,
                &["*.ts".to_owned()],
                &|| false,
                Duration::from_secs(60),
            )
            .expect("fingerprint")
        };
        let first = digest();
        write(&path, "export const a = 2;\n");
        set_old_time();
        assert_ne!(digest(), first);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn text_only_files_are_the_scanned_files_the_answer_misses() {
        let (_dir, root) = fixture();
        write(&root.join("a.ts"), "export function f() {}\n");
        write(&root.join("b.ts"), "import { f } from './a';\nf();\n");
        write(&root.join("c.ts"), "// f is mentioned here\n");
        write(&root.join("d.rs"), "fn f() {}\n");
        let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
        let policy = workspace_policy(&root);
        let cancel = crate::tools::cancel::NeverCancel;
        let files = scope
            .text_files("f", &policy, &cancel)
            .await
            .expect("scan")
            .expect("files");
        assert_eq!(files.len(), 3, "{files:?}");
        // Unknown until the operation records its answer.
        assert_eq!(scope.text_only_files("f"), None);
        scope.answer([root.join("a.ts").to_string_lossy()]);
        scope.answer([format!("file://{}", root.join("b.ts").display())]);
        assert_eq!(
            scope.text_only_files("f"),
            Some(vec![root.join("c.ts").to_string_lossy().into_owned()])
        );
        assert_eq!(scope.text_only_files("g"), None);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn text_scan_is_reused_under_an_unchanged_fingerprint() {
        let (_dir, root) = fixture();
        write(&root.join("a.ts"), "export function f() {}\n");
        write(&root.join("b.ts"), "f();\n");
        let policy = workspace_policy(&root);
        let cancel = &crate::tools::cancel::NeverCancel;
        let include = vec!["*.ts".to_owned()];
        let scan = |fingerprint: Option<&str>| {
            let scope = Scope::new(root.to_string_lossy().into_owned(), include.clone());
            if let Some(fingerprint) = fingerprint {
                scope.set_fingerprint(fingerprint.to_owned());
            }
            let policy = policy.clone();
            async move {
                scope
                    .text_files("f", &policy, cancel)
                    .await
                    .expect("scan")
                    .expect("files")
                    .len()
            }
        };
        assert_eq!(scan(Some("fp")).await, 2);
        // A new mention under the same fingerprint string: the earlier
        // scan answers (the fingerprint is what proves the files unchanged),
        // so no file is read again.
        write(&root.join("c.ts"), "f();\n");
        assert_eq!(scan(Some("fp")).await, 2);
        // Another fingerprint, or none (walk past its bounds), scans again.
        assert_eq!(scan(Some("fp2")).await, 3);
        write(&root.join("d.ts"), "f();\n");
        assert_eq!(scan(None).await, 4);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn canonical_paths_are_memoized_per_request() {
        let (_dir, root) = fixture();
        write(&root.join("one/x.ts"), "x\n");
        write(&root.join("two/x.ts"), "x\n");
        let link = root.join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("one"), &link).expect("link");
        #[cfg(not(unix))]
        return;
        let spelled = link.join("x.ts").to_string_lossy().into_owned();
        let first = root.join("one/x.ts").to_string_lossy().into_owned();
        let second = root.join("two/x.ts").to_string_lossy().into_owned();
        let seen = with_canonical_memo(async {
            let before = canonical(&spelled);
            std::fs::remove_file(&link).expect("unlink");
            #[cfg(unix)]
            std::os::unix::fs::symlink(root.join("two"), &link).expect("relink");
            (before, canonical(&spelled))
        })
        .await;
        // One request resolves a spelling once; the next request sees the
        // repointed link.
        assert_eq!(seen, (first.clone(), first));
        assert_eq!(
            with_canonical_memo(async { canonical(&spelled) }).await,
            second
        );
        assert_eq!(canonical(&spelled), second);
    }
}
