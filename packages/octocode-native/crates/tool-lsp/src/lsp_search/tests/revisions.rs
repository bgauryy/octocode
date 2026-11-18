//! Source revisions a continuation re-proves: the anchor text and the scope
//! as its first page observed them.

use super::*;
use octocode_engine::lsp::client::{RESPONSE_SCOPE, ResponseScope};
use std::time::{Duration, SystemTime};

/// The revision one page proves under `pinned` (empty: a fresh request),
/// and whether it matches.
async fn page(
    query: &LspSearchQuery,
    root: &std::path::Path,
    paths: &octocode_runtime_base::policy::path::PathPolicy,
    pinned: &str,
) -> (bool, String) {
    let anchor = root.join("a.ts");
    let scope = scope::Scope::resolve(
        &anchor.to_string_lossy(),
        &root.to_string_lossy(),
        None,
        paths,
        Some("typescript"),
        &octocode_engine::lsp::config::LspDiscoveryOptions::default(),
    );
    let document = source::Source::new(std::fs::read_to_string(&anchor).expect("anchor"));
    let pinned = ResponseScope {
        generation: pinned.to_owned(),
        ..ResponseScope::default()
    };
    let started = SystemTime::now();
    RESPONSE_SCOPE
        .scope(std::cell::RefCell::new(pinned), async {
            if RESPONSE_SCOPE.with(|scope| scope.borrow().generation.is_empty()) {
                // Model the watching server's observation before provider reads.
                // Recently written sources cannot be proved by a late seal alone.
                scope
                    .walked(
                        SystemTime::now(),
                        &octocode_runtime_base::execution::cancel::NeverCancel,
                    )
                    .await
                    .expect("pre-read walk")
                    .fingerprint
                    .expect("complete source proof");
            }
            let current = scope_responses(
                query,
                &scope,
                Some(&document),
                &octocode_runtime_base::execution::cancel::NeverCancel,
            )
            .await
            .expect("revision");
            // A fresh page that issues a continuation pins the scope it
            // observes when it finishes.
            seal_scope_revision(
                query,
                &scope,
                Some(&document),
                started,
                &octocode_runtime_base::execution::cancel::NeverCancel,
            )
            .await
            .expect("seal");
            (
                current,
                RESPONSE_SCOPE.with(|scope| scope.borrow().generation.clone()),
            )
        })
        .await
}

/// A large source is fingerprinted at the first page's instant. Its
/// continuation stays current after the file settles, but not after removal.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_continuation_preserves_a_large_source_revision_after_it_settles() {
    let (_dir, root, paths) = temp_workspace();
    std::fs::write(root.join("a.ts"), "export function f() {}\n").expect("anchor");
    let large = root.join("large.ts");
    std::fs::write(&large, vec![b'x'; 2 * 1024 * 1024]).expect("large");
    // Unsettled now, settled one second from now.
    let modified = SystemTime::now() - scope::FRESH_MTIME + Duration::from_secs(1);
    std::fs::File::options()
        .write(true)
        .open(&large)
        .expect("large")
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .expect("mtime");
    let references = query(serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "f", "line": 1
    }));
    let (fresh, first) = page(&references, &root, &paths, "").await;
    assert!(fresh);
    let settled = modified + scope::FRESH_MTIME + Duration::from_millis(100);
    if let Ok(wait) = settled.duration_since(SystemTime::now()) {
        tokio::time::sleep(wait).await;
    }
    let (current, second) = page(&references, &root, &paths, &first).await;
    assert!(
        current,
        "an unchanged scope stays current: {first} vs {second}"
    );
    std::fs::remove_file(&large).expect("remove");
    assert!(
        !page(&references, &root, &paths, &first).await.0,
        "a scope that changed since the first page is stale"
    );
}

/// The revision a fresh page seals after its server cited `cited`, with
/// `cited` removed (or kept) between the answer and the seal.
async fn sealed_after_citing(
    root: &std::path::Path,
    paths: &octocode_runtime_base::policy::path::PathPolicy,
    query: &LspSearchQuery,
    remove: bool,
) -> String {
    let anchor = root.join("a.ts");
    let cited = root.join("b.ts");
    std::fs::write(&cited, "f();\n").expect("cited");
    let scope = scope::Scope::resolve(
        &anchor.to_string_lossy(),
        &root.to_string_lossy(),
        None,
        paths,
        Some("typescript"),
        &octocode_engine::lsp::config::LspDiscoveryOptions::default(),
    );
    let document = source::Source::new(std::fs::read_to_string(&anchor).expect("anchor"));
    octocode_engine::stamp::settle_written_so_far();
    let started = octocode_engine::stamp::settle_clock();
    scope.answer([cited.to_string_lossy()]);
    if remove {
        std::fs::remove_file(&cited).expect("remove");
    }
    RESPONSE_SCOPE
        .scope(std::cell::RefCell::new(ResponseScope::default()), async {
            seal_scope_revision(
                query,
                &scope,
                Some(&document),
                started,
                &octocode_runtime_base::execution::cancel::NeverCancel,
            )
            .await
            .expect("seal");
            RESPONSE_SCOPE.with(|scope| scope.borrow().generation.clone())
        })
        .await
}

/// A cited file removed after the server read it, before the page seals,
/// leaves no stamp for the sealing walk (nor the next page's) to compare:
/// the revision is unsettled, so the continuation refuses instead of
/// continuing over rows from a file that is gone. Kept, it stays current.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cited_file_removed_before_the_seal_stales_the_continuation() {
    let (_dir, root, paths) = temp_workspace();
    std::fs::write(root.join("a.ts"), "export function f() {}\n").expect("anchor");
    let references = query(serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "f", "line": 1
    }));
    let kept = sealed_after_citing(&root, &paths, &references, false).await;
    assert!(
        page(&references, &root, &paths, &kept).await.0,
        "kept: {kept}"
    );
    let removed = sealed_after_citing(&root, &paths, &references, true).await;
    assert!(
        !page(&references, &root, &paths, &removed).await.0,
        "removed: {removed}"
    );
}

/// The continuation revision and the watched-file diff share the request's
/// one walk, whatever instant each would have observed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_revision_and_the_watched_diff_share_one_walk() {
    use scope::probe::{Work, measure};
    let (_dir, root, paths) = temp_workspace();
    std::fs::write(root.join("a.ts"), "export function f() {}\n").expect("anchor");
    let scope = scope::Scope::resolve(
        &root.join("a.ts").to_string_lossy(),
        &root.to_string_lossy(),
        None,
        &paths,
        Some("typescript"),
        &octocode_engine::lsp::config::LspDiscoveryOptions::default(),
    );
    let work = std::sync::Arc::new(Work::default());
    let never = octocode_runtime_base::execution::cancel::NeverCancel;
    let (first, second) = measure(&work, async {
        let first = scope.walked(SystemTime::now(), &never).await.expect("walk");
        let later = SystemTime::now() + Duration::from_secs(60);
        (first, scope.walked(later, &never).await.expect("reuse"))
    })
    .await;
    assert_eq!(Work::get(&work.walks), 1);
    assert_eq!(first.observed, second.observed);
    assert!(first.fingerprint.is_some());
}

/// A write after a page began is seen by its stamp, and so is one inside the
/// stamp's settle window before it (a coarse clock may date a later write
/// early): whole seconds wait two, a fine stamp twice the clock tick.
#[test]
fn written_since_reads_the_last_write_of_each_stamp() {
    let started = std::time::UNIX_EPOCH + Duration::new(1_700_000_000, 500_000_000);
    let ns = |secs: u64, nanos: u64| secs * 1_000_000_000 + nanos;
    let entries = |modified: u64, changed: u64| {
        std::collections::BTreeMap::from([(
            "a.ts".to_owned(),
            format!("3\u{0}{modified}\u{0}{changed}\u{0}9\u{0}"),
        )])
    };
    let before = ns(1_700_000_000, 400_000_001);
    let after = ns(1_700_000_000, 600_000_001);
    assert!(!scope::written_since(&entries(before, before), started));
    assert!(
        scope::written_since(&entries(before, after), started),
        "ctime"
    );
    assert!(
        scope::written_since(&entries(after, before), started),
        "mtime"
    );
    let window = octocode_engine::stamp::SourceStamp::COARSE_CLOCK_TICK * 2;
    let window_ns = u64::try_from(window.as_nanos()).expect("window");
    let started_ns = ns(1_700_000_000, 500_000_000);
    let inside = started_ns - window_ns + 1;
    assert!(
        scope::written_since(&entries(inside, inside), started),
        "a fine stamp within its window of the start"
    );
    let outside = started_ns - window_ns - 1;
    assert!(!scope::written_since(&entries(outside, outside), started));
    let whole = ns(1_699_999_999, 0);
    assert!(
        scope::written_since(&entries(whole, whole), started),
        "a coarse stamp within the settle window"
    );
    let settled = ns(1_699_999_990, 0);
    assert!(!scope::written_since(&entries(settled, settled), started));
    let unparsable = std::collections::BTreeMap::from([("a.ts".to_owned(), "x".to_owned())]);
    assert!(scope::written_since(&unparsable, started));
}

/// A file that settles between two walks keeps its identity for the watched
/// diff: its stamp loses the content hash it carried while unsettled, yet it
/// is the same file; a hash that no longer matches its content, or an edit,
/// is a change.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_that_settles_between_walks_is_not_a_watched_change() {
    let (_dir, root, paths) = temp_workspace();
    let path = root.join("a.ts");
    let modified = std::time::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    let write = |text: &str| {
        std::fs::write(&path, text).expect("anchor");
        std::fs::File::options()
            .write(true)
            .open(&path)
            .expect("anchor")
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .expect("mtime");
    };
    write("export function f() {}\n");
    let scope = scope::Scope::resolve(
        &path.to_string_lossy(),
        &root.to_string_lossy(),
        None,
        &paths,
        Some("typescript"),
        &octocode_engine::lsp::config::LspDiscoveryOptions::default(),
    );
    let stamp = |observed| {
        let scope = &scope;
        async move {
            let (_, entries) = scope
                .walk_fingerprint(
                    observed,
                    &octocode_runtime_base::execution::cancel::NeverCancel,
                )
                .await
                .expect("walk")
                .expect("fingerprint");
            entries["a.ts"].clone()
        }
    };
    let unsettled = stamp(modified + scope::FRESH_MTIME / 2).await;
    let settled = stamp(modified + scope::FRESH_MTIME * 2).await;
    assert_ne!(
        unsettled, settled,
        "the stamps differ by their walk's instant"
    );
    assert!(scope::same_file(&path, &unsettled, &settled, &|| false));
    assert!(scope::same_file(&path, &settled, &unsettled, &|| false));
    let (metadata, _) = unsettled.rsplit_once('\u{0}').expect("stamp");
    let other = format!(
        "{metadata}\u{0}{}",
        octocode_engine::digest::sha256(b"other")
    );
    assert!(
        !scope::same_file(&path, &other, &settled, &|| false),
        "content changed"
    );
    write("export function g() {}\n");
    let edited = stamp(modified + scope::FRESH_MTIME * 2).await;
    assert!(
        !scope::same_file(&path, &settled, &edited, &|| false),
        "an edit"
    );
}
