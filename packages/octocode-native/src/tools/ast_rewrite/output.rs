//! Response-shaping helpers: build the tool output Value from prepared rewrite data.
use serde_json::{Map, Value, json};
use std::path::Path;
use super::{AstRewriteQuery, ExecutableReceipt, PreparedFile, PreparedMatch, RewriteError};

pub(super) fn success_value(
    query: &AstRewriteQuery,
    root: &Path,
    snapshot: &str,
    files: &[PreparedFile],
    matches: &[PreparedMatch],
    executable: &ExecutableReceipt,
    transaction: Option<Value>,
) -> Value {
    let apply = query.apply;
    let page = if apply { 1 } else { query.page };
    let page_size = if apply {
        matches.len().max(1)
    } else {
        query.page_size
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
    let mut value = json!({
        "operation":"rewrite","mode":if apply {"apply"} else {"preview"},
        "root":root,"snapshot":snapshot,"executable":executable_value(executable),
        "isolation":isolation_receipt(),"totalMatches":matches.len(),
        "affectedFiles":files.len(),
        "matches":shown.iter().map(|matched| matched.public.clone()).collect::<Vec<_>>(),
        "files":files.iter().map(public_file).collect::<Vec<_>>(),
        "complete":!has_more,"isPartial":has_more,
        "pagination":{"currentPage":page,"totalPages":total_pages,"pageSize":page_size,"hasMore":has_more}
    });
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
    if let Some(transaction) = transaction {
        value["transaction"] = transaction;
    }
    value
}

pub(super) fn continuation_query(query: &AstRewriteQuery, canonical_root: &Path) -> Value {
    let mut value = Map::new();
    if let Some(goal) = &query.goal {
        value.insert("goal".to_owned(), json!(goal));
    }
    if let Some(reasoning) = &query.reasoning {
        value.insert("reasoning".to_owned(), json!(reasoning));
    }
    value.insert("path".to_owned(), json!(canonical_root));
    value.insert("langType".to_owned(), json!(query.lang_type));
    value.insert("apply".to_owned(), json!(query.apply));
    value.insert("maxFiles".to_owned(), json!(query.max_files));
    value.insert("maxMatches".to_owned(), json!(query.max_matches));
    value.insert("page".to_owned(), json!(query.page));
    value.insert("pageSize".to_owned(), json!(query.page_size));
    value.insert(
        "ruleKind".to_owned(),
        json!(query.rule_kind.as_deref().unwrap_or("pattern")),
    );
    for (key, item) in [
        ("pattern", query.pattern.as_ref().map(|value| json!(value))),
        ("rewrite", query.rewrite.as_ref().map(|value| json!(value))),
        ("rule", query.rule.clone()),
        ("constraints", query.constraints.clone()),
        ("utils", query.utils.clone()),
        ("transform", query.transform.clone()),
        ("fix", query.fix.clone()),
        ("rewriters", query.rewriters.clone()),
        ("include", query.include.as_ref().map(|value| json!(value))),
        ("exclude", query.exclude.as_ref().map(|value| json!(value))),
        (
            "expectedHashes",
            query.expected_hashes.as_ref().map(|value| json!(value)),
        ),
        (
            "selectedMatchIds",
            query.selected_match_ids.as_ref().map(|value| json!(value)),
        ),
        (
            "postconditions",
            query.postconditions.as_ref().map(|value| json!(value)),
        ),
        (
            "snapshot",
            query.snapshot.as_ref().map(|value| json!(value)),
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
    json!({
        "path":executable.path,
        "version":executable.version,
        "sha256":executable.sha256,
        "capabilityContract":1,
        "capabilityDigest":executable.capability_digest,
        "capabilities":executable.capabilities
    })
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

