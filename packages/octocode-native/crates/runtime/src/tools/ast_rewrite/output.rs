//! Response-shaping helpers: build the tool output Value from prepared rewrite data.
use super::{ExecutableReceipt, PreparedFile, PreparedMatch, RewriteError, RewriteRequest};
use serde_json::{Map, Value, json};
use std::path::Path;

pub(super) fn success_value(
    query: &RewriteRequest,
    root: &Path,
    snapshot: &str,
    files: &[PreparedFile],
    matches: &[PreparedMatch],
    executable: &ExecutableReceipt,
    transaction: Option<Value>,
) -> Value {
    let apply = query.apply();
    let page = if apply { 1 } else { query.page() };
    let page_size = if apply {
        matches.len().max(1)
    } else {
        query.page_size()
    };
    let offset = page.saturating_sub(1).saturating_mul(page_size);
    let shown = if apply {
        matches
    } else {
        matches
            .get(offset..offset.saturating_add(page_size).min(matches.len()))
            .unwrap_or(&[])
    };
    let has_more = !apply && offset.saturating_add(page_size) < matches.len();
    let total_pages = matches.len().div_ceil(page_size).max(1);
    // A preview page carries only the files its matches touch. A file whose
    // matches span pages sends its whole-file patch once, on the first page
    // touching it; later pages reference that page (`patchOnPage`) instead of
    // resending it. `affectedFiles` stays the full count and the final page's
    // `next.apply` still guards every affected file.
    let page_paths = shown
        .iter()
        .filter_map(|matched| matched.public["path"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut first_page = std::collections::BTreeMap::<&str, usize>::new();
    for (index, matched) in matches.iter().enumerate() {
        if let Some(path) = matched.public["path"].as_str() {
            first_page.entry(path).or_insert(index / page_size + 1);
        }
    }
    let page_files = files
        .iter()
        .filter(|file| apply || page_paths.contains(file.path.as_str()))
        .map(|file| {
            let mut value = public_file(file);
            if let Some(&first) = first_page
                .get(file.path.as_str())
                .filter(|first| !apply && **first < page)
                && let Some(object) = value.as_object_mut()
            {
                object.remove("patch");
                object.insert("patchOnPage".to_owned(), json!(first));
            }
            value
        })
        .collect::<Vec<_>>();
    let mut value = json!({
        "operation":"rewrite","mode":if apply {"apply"} else {"preview"},
        "root":root,"snapshot":snapshot,"totalMatches":matches.len(),
        "affectedFiles":files.len(),
        "matches":shown.iter().map(|matched| matched.public.clone()).collect::<Vec<_>>(),
        "files":page_files,
        "complete":!has_more,"isPartial":has_more,
        "pagination":{"currentPage":page,"totalPages":total_pages,"pageSize":page_size,"hasMore":has_more}
    });
    attach_receipts(&mut value, query, executable);
    if has_more {
        let mut next = continuation_query(query, root);
        next["apply"] = json!(false);
        next["page"] = json!(page + 1);
        next["pageSize"] = json!(page_size);
        next["snapshot"] = json!(snapshot);
        value["next"] = json!({"nextPage":{
            "tool":"astRewrite","query":next,"confidence":"exact"
        }});
    }
    // A complete preview carries its own guarded apply: the same query with the
    // preview snapshot and every file's beforeHash. Running it stays the
    // caller's decision; changed sources or selections are still rejected.
    if !apply && !has_more && !matches.is_empty() {
        let mut next = continuation_query(query, root);
        next["apply"] = json!(true);
        next["page"] = json!(1);
        next["snapshot"] = json!(snapshot);
        next["expectedHashes"] = Value::Object(
            files
                .iter()
                .map(|file| (file.path.clone(), json!(file.before_hash)))
                .collect(),
        );
        value["next"]["apply"] = json!({
            "tool":"astRewrite","query":next,"confidence":"exact",
            "why":"Apply exactly this preview; changed sources or selections are rejected."
        });
    }
    if let Some(transaction) = transaction {
        value["transaction"] = transaction;
    }
    value
}

pub(super) fn continuation_query(query: &RewriteRequest, canonical_root: &Path) -> Value {
    let mut value = Map::new();
    if let Some(goal) = &query.goal() {
        value.insert("goal".to_owned(), json!(goal));
    }
    value.insert("reasoning".to_owned(), json!(query.reasoning()));
    value.insert("path".to_owned(), json!(canonical_root));
    value.insert("langType".to_owned(), json!(query.lang_type()));
    value.insert("apply".to_owned(), json!(query.apply()));
    value.insert("maxFiles".to_owned(), json!(query.max_files()));
    value.insert("maxMatches".to_owned(), json!(query.max_matches()));
    value.insert("page".to_owned(), json!(query.page()));
    value.insert("pageSize".to_owned(), json!(query.page_size()));
    value.insert("ruleKind".to_owned(), json!(query.rule_kind()));
    for (key, item) in [
        (
            "pattern",
            query.pattern().as_ref().map(|value| json!(value)),
        ),
        (
            "rewrite",
            query.rewrite().as_ref().map(|value| json!(value)),
        ),
        ("rule", query.rule().cloned()),
        ("constraints", query.constraints().cloned()),
        ("utils", query.utils().cloned()),
        ("transform", query.transform().cloned()),
        ("fix", query.fix().cloned()),
        (
            "include",
            query.include().as_ref().map(|value| json!(value)),
        ),
        (
            "exclude",
            query.exclude().as_ref().map(|value| json!(value)),
        ),
        (
            "expectedHashes",
            query.expected_hashes().as_ref().map(|value| json!(value)),
        ),
        (
            "selectedMatchIds",
            query
                .selected_match_ids()
                .as_ref()
                .map(|value| json!(value)),
        ),
        (
            "postconditions",
            query.postconditions().as_ref().map(|value| json!(value)),
        ),
        (
            "snapshot",
            query.snapshot().as_ref().map(|value| json!(value)),
        ),
    ] {
        if let Some(item) = item {
            value.insert(key.to_owned(), item);
        }
    }
    Value::Object(value)
}

pub(super) fn public_file(file: &PreparedFile) -> Value {
    json!({
        "path":file.path,"absolutePath":file.absolute,"beforeHash":file.before_hash,
        "afterHash":file.after_hash,"matchCount":file.matches.len(),
        "patch":file.patch,"patchBytes":file.patch.len()
    })
}

pub(super) fn executable_value(executable: &ExecutableReceipt) -> Value {
    let mut value = json!({
        "path":executable.path,
        "version":executable.version,
        "capabilityContract":1,
        "capabilityDigest":executable.capability_digest,
        "capabilities":executable.capabilities
    });
    if !executable.sha256.is_empty() {
        value["sha256"] = json!(executable.sha256);
    }
    value
}

/// The executable and isolation receipts are diagnostics: the snapshot digest
/// already binds the executable, and nothing a caller does next depends on
/// them, so they ride only on `debug`.
pub(super) fn attach_receipts(
    value: &mut Value,
    query: &RewriteRequest,
    executable: &ExecutableReceipt,
) {
    if query.debug() {
        value["executable"] = executable_value(executable);
        value["isolation"] = isolation_receipt();
    }
}

pub(super) fn isolation_receipt() -> Value {
    json!({
        "workingDirectory":"ephemeral","inheritedHome":false,
        "repositoryConfig":"not-discovered"
    })
}

pub(super) fn portable_relative(root: &Path, target: &Path) -> Result<String, RewriteError> {
    target
        .strip_prefix(root)
        .map(|path| {
            path.to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")
        })
        .map_err(|_| {
            RewriteError::new(
                "ast.rewrite.path_escape",
                "The rewrite escaped the requested root.",
            )
        })
}
