//! One spelling for local evidence paths before shared response shaping.
use crate::policy::path::PathPolicy;
use serde_json::Value;

/// Keep executable requests, provider URIs and source text unchanged. Only
/// evidence paths inside the workspace become relative to that workspace.
pub(crate) fn workspace_relative(value: &mut Value, paths: &PathPolicy) {
    match value {
        Value::Object(fields) => {
            if matches!(
                fields.get("kind").and_then(Value::as_str),
                Some("continue" | "related")
            ) && fields.get("tool").is_some_and(Value::is_string)
                && fields.get("input").is_some_and(Value::is_object)
            {
                return;
            }
            for (key, value) in fields {
                if matches!(key.as_str(), "path" | "workspaceRoot")
                    && let Some(relative) = value
                        .as_str()
                        .and_then(|path| paths.workspace_relative(path))
                {
                    *value = Value::String(relative);
                } else {
                    workspace_relative(value, paths);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                workspace_relative(item, paths);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn paths_align_without_rewriting_source_provider_identity_or_executable_requests() {
        let dir = tempfile::tempdir().expect("workspace");
        let root = dir.path().canonicalize().expect("canonical workspace");
        let paths = crate::tools::test_support::workspace_policy(&root);
        let file = root.join("src/lib.ts").to_string_lossy().into_owned();
        let command = root.join("server").to_string_lossy().into_owned();
        let uri = octocode_engine::lsp::uri::path_to_uri(&file).expect("URI");
        let source = format!("1\tconst path = {file:?};\n");
        let related =
            json!({"kind":"related","tool":"localFetch","input":{"queries":[{"path":file}]}});
        let retry = json!({"kind":"continue","tool":"lspSearch","input":{"queries":[{"path":file,"operation":"documentSymbols"}]}});
        let mut row = json!({
            "path":file, "workspaceRoot":root,
            "lsp":{"receipt":{"workspaceRoot":root,"command":command,"argv":[file]}},
            "matches":[{"path":file,"content":source,"uri":uri},{"path":"/external/library.ts"}],
            "files":[{"path":file,"matches":[{"sites":[{"path":file}]}]}],
            "symbols":[{"path":file,"children":[{"kind":"related","path":file}]}],
            "diagnostics":[{"relatedInformation":[{"location":{"path":file}}]}],
            "skippedPaths":{"entries":[{"path":file}]},
            "related":[related],"next":{"retry":retry}
        });
        workspace_relative(&mut row, &paths);
        for pointer in [
            "/path",
            "/matches/0/path",
            "/files/0/path",
            "/files/0/matches/0/sites/0/path",
            "/symbols/0/path",
            "/symbols/0/children/0/path",
            "/diagnostics/0/relatedInformation/0/location/path",
            "/skippedPaths/entries/0/path",
        ] {
            assert_eq!(
                row.pointer(pointer),
                Some(&json!("src/lib.ts")),
                "{pointer}: {row}"
            );
        }
        assert_eq!(row["workspaceRoot"], ".");
        assert_eq!(row["lsp"]["receipt"]["workspaceRoot"], ".");
        assert_eq!(row["matches"][1]["path"], "/external/library.ts");
        assert_eq!(row["matches"][0]["content"], source);
        assert_eq!(row["matches"][0]["uri"], uri);
        assert_eq!(row["lsp"]["receipt"]["command"], command);
        assert_eq!(row["lsp"]["receipt"]["argv"][0], file);
        assert_eq!(row["related"][0], related);
        assert_eq!(row["next"]["retry"], retry);
        let normalized = row.clone();
        workspace_relative(&mut row, &paths);
        assert_eq!(row, normalized, "already-relative pages are stable");
    }
}
