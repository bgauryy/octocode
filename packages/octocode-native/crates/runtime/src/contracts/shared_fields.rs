//! Leaf fields a response hoists into `shared` when every row entry holds
//! the same value.
//!
//! `shared` holds two kinds of keys: entry keys (hoisted from every object
//! entry of every array under `data`) and [`ROW_FIELDS`] (stated once for
//! every row, such as the checked-out head). A field an entry schema of the
//! responding tool requires is never hoisted, so the returned JSON validates
//! as it is: the output contract checks exactly what a client receives.
use crate::id::ToolId;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// Row-level fields a response may state once in `shared`. They are never
/// hoisted from entries, so a shared key has exactly one origin.
pub(crate) const ROW_FIELDS: &[&str] = &["commitSha"];

fn can_share_field(tool: ToolId, key: &str, value: &Value) -> bool {
    const EXCLUDED: &[&str] = &[
        "path",
        // A row's file, like its path, says what the row is about: it stays
        // on every row so no row is read against a trailing `shared` map.
        "file",
        "dir",
        "uri",
        "absolutePath",
        "owner",
        "repo",
        "name",
        // A declaration's name is its lspSearch anchor: every symbols row
        // keeps it.
        "symbolName",
        "id",
        "type",
        "kind",
        "reason",
        "isPartial",
        "number",
        "title",
        "state",
        "author",
        "labels",
        "createdAt",
        "mergedAt",
        "commentsCount",
        "startLine",
        "endLine",
        "start",
        "end",
        "startColumn",
        "endColumn",
        "startByte",
        "endByte",
        "line",
        "column",
        "character",
        "parentId",
        "parent",
        "named",
        "exported",
        // Per-entry match-row accounting must stay on each entry: hoisting it
        // whenever the values happen to coincide (typical on page 1) makes the
        // row shape depend on the data, so identical queries drift between
        // pages and between CLI and MCP consumers.
        "totalMatchRows",
        "returnedMatchRows",
        // Per-file counts are the count views' evidence: equal counts on a
        // page (common on later pages) must not move into `shared`.
        "matchCount",
        "matchedLineCount",
    ];
    !EXCLUDED.contains(&key)
        && !ROW_FIELDS.contains(&key)
        && !required_entry_fields(tool).contains(key)
        && (value.is_number()
            || value.is_boolean()
            || value.as_str().is_some_and(|s| !s.is_empty()))
}

/// Every field some array-entry schema of `tool`'s output contract
/// requires: hoisting one would remove it from entries whose schema demands
/// it. Derived once per tool from that tool's contract entry.
pub(crate) fn required_entry_fields(tool: ToolId) -> &'static BTreeSet<String> {
    static REQUIRED: PerTool = [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    per_tool(&REQUIRED, tool, |schema| {
        let mut required = BTreeSet::new();
        let mut walk = EntryWalk {
            defs: &schema["$defs"],
            seen: std::collections::HashSet::new(),
            required: &mut required,
        };
        walk.visit(schema, false);
        required
    })
}

fn required_row_fields(tool: ToolId) -> &'static BTreeSet<String> {
    static REQUIRED: PerTool = [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    per_tool(&REQUIRED, tool, |schema| {
        let defs = &schema["$defs"];
        let mut required = BTreeSet::new();
        for array in super::property_schemas(schema, defs, tool.output().rows_key()) {
            for row in super::item_schemas(array, defs) {
                for data in super::property_schemas(row, defs, "data") {
                    let mut walk = EntryWalk {
                        defs,
                        seen: std::collections::HashSet::new(),
                        required: &mut required,
                    };
                    walk.visit(data, true);
                }
            }
        }
        required
    })
}

/// One field table per tool, indexed by [`ToolId::index`].
type PerTool = [std::sync::OnceLock<BTreeSet<String>>; ToolId::ALL.len()];

/// `tool`'s table in `tables`, built on first use by `build` from that
/// tool's output schema only (see [`super::tool_contract`]): a response
/// never parses the contracts of tools it does not answer for. A tool the
/// contract lacks has no fields.
fn per_tool(
    tables: &'static PerTool,
    tool: ToolId,
    build: impl FnOnce(&Value) -> BTreeSet<String>,
) -> &'static BTreeSet<String> {
    tables[tool.index()].get_or_init(|| {
        super::tool_contract(tool)
            .map(|contract| build(&contract["outputSchema"]))
            .unwrap_or_default()
    })
}

/// One output schema's walk: inside an array's `items` (through references
/// and union branches) it collects each object's `required` names. Each
/// schema node is visited once per context, so recursive definitions end.
struct EntryWalk<'a, 'b> {
    defs: &'a Value,
    seen: std::collections::HashSet<(*const Value, bool)>,
    required: &'b mut BTreeSet<String>,
}

impl<'a> EntryWalk<'a, '_> {
    fn visit(&mut self, schema: &'a Value, entry: bool) {
        let schema = super::resolve_ref(schema, self.defs);
        if !self.seen.insert((std::ptr::from_ref(schema), entry)) {
            return;
        }
        let Some(object) = schema.as_object() else {
            return;
        };
        if entry {
            self.required.extend(
                object
                    .get("required")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
        }
        for (key, value) in object {
            match key.as_str() {
                "items" => self.visit(value, true),
                "oneOf" | "anyOf" | "allOf" => {
                    for branch in value.as_array().into_iter().flatten() {
                        self.visit(branch, entry);
                    }
                }
                "properties" | "patternProperties" => {
                    for property in value.as_object().into_iter().flat_map(|map| map.values()) {
                        self.visit(property, false);
                    }
                }
                "additionalProperties" => self.visit(value, false),
                _ => {}
            }
        }
    }
}

fn shared_leaves_mut(rows: &mut [Value]) -> impl Iterator<Item = &mut Map<String, Value>> {
    rows.iter_mut()
        .filter_map(|row| row["data"].as_object_mut())
        .flat_map(|data| data.values_mut().filter_map(Value::as_array_mut))
        .flatten()
        .filter_map(Value::as_object_mut)
}

/// Hoist leaf fields every row entry of `tool`'s response shares into one
/// `shared` map.
pub(crate) fn hoist(tool: ToolId, rows: &mut [Value]) -> Option<Map<String, Value>> {
    let mut shared = Map::new();
    if rows.len() > 1 {
        for &key in ROW_FIELDS {
            if required_row_fields(tool).contains(key) {
                continue;
            }
            let Some(value) = rows[0]["data"]
                .get(key)
                .filter(|value| value.as_str().is_some_and(|text| !text.is_empty()))
                .cloned()
            else {
                continue;
            };
            if rows.iter().all(|row| row["data"].get(key) == Some(&value)) {
                for row in rows.iter_mut() {
                    if let Some(data) = row["data"].as_object_mut() {
                        data.shift_remove(key);
                    }
                }
                shared.insert(key.into(), value);
            }
        }
    }
    let leaves: Vec<&Map<String, Value>> = rows
        .iter()
        .filter_map(|row| row["data"].as_object())
        .flat_map(|data| data.values().filter_map(Value::as_array))
        .flatten()
        .filter_map(Value::as_object)
        .collect();
    if leaves.len() < 2 {
        return (!shared.is_empty()).then_some(shared);
    }
    let entry_shared: Map<String, Value> = leaves[0]
        .iter()
        .filter(|(key, value)| {
            can_share_field(tool, key, value)
                && leaves.iter().all(|leaf| leaf.get(*key) == Some(*value))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    for leaf in shared_leaves_mut(rows) {
        for key in entry_shared.keys() {
            // `remove` swaps the last key into the gap; the entry keeps the
            // order the tool wrote.
            leaf.shift_remove(key);
        }
    }
    shared.extend(entry_shared);
    (!shared.is_empty()).then_some(shared)
}

#[cfg(test)]
mod tests {
    use crate::contracts::validate_output;
    use crate::id::ToolId;
    use serde_json::json;

    #[test]
    fn repeated_row_commits_share_once_without_changing_file_evidence() {
        let sha = "a".repeat(40);
        let rows = vec![
            json!({"index":0,"data":{"path":"a.rs","commitSha":sha,"content":"1\ta\n","totalLines":1}}),
            json!({"index":1,"data":{"path":"b.rs","commitSha":sha,"content":"1\tb\n","totalLines":1}}),
        ];
        let original = json!({"results":rows});
        let compact = crate::response::rows::envelope(ToolId::GhGetFileContent, rows.clone());
        validate_output("ghGetFileContent", &compact).expect("the returned wire validates");
        assert_eq!(compact["shared"]["commitSha"], sha);
        assert!(compact.to_string().len() < original.to_string().len());
        for (returned, expected) in compact["results"]
            .as_array()
            .expect("rows")
            .iter()
            .zip(&rows)
        {
            assert!(returned["data"].get("commitSha").is_none());
            let mut restored = returned.clone();
            restored["data"]["commitSha"] = compact["shared"]["commitSha"].clone();
            assert_eq!(restored, *expected, "every original field is recoverable");
        }
        for replacement in [json!("b".repeat(40)), serde_json::Value::Null] {
            let mut distinct = rows.clone();
            distinct[1]["data"]["commitSha"] = replacement;
            let before = distinct.clone();
            assert!(super::hoist(ToolId::GhGetFileContent, &mut distinct).is_none());
            assert_eq!(
                distinct, before,
                "different or missing revisions stay on their rows"
            );
        }
    }

    #[test]
    fn required_revision_identity_stays_on_each_row() {
        for tool in [ToolId::GhStructure, ToolId::GhGetHistoryItem] {
            let mut rows = vec![
                json!({"index":0,"data":{"commitSha":"same"}}),
                json!({"index":1,"data":{"commitSha":"same"}}),
            ];
            let original = rows.clone();
            assert!(super::hoist(tool, &mut rows).is_none());
            assert_eq!(rows, original);
            validate_output(tool.as_str(), &json!({"results":rows}))
                .expect("required commit identity remains on the wire");
        }
    }

    /// A count is per-file evidence: equal counts on one page stay on each
    /// row, so a count view reads the same on every page (no `shared` lookup).
    #[test]
    fn per_file_counts_are_never_hoisted() {
        let mut rows = vec![json!({"index":0,"data":{"files":[
            {"path":"a.rs","matchCount":9,"matchedLineCount":9,"note":"same"},
            {"path":"b.rs","matchCount":9,"matchedLineCount":9,"note":"same"}
        ]}})];
        let shared = super::hoist(ToolId::LocalSearch, &mut rows)
            .expect("an identical non-count field still hoists");
        assert!(!shared.contains_key("matchCount") && !shared.contains_key("matchedLineCount"));
        assert_eq!(rows[0]["data"]["files"][1]["matchCount"], 9);
        assert_eq!(rows[0]["data"]["files"][0]["matchedLineCount"], 9);
    }

    /// A row's `file` is the identity of what it names, like `path` and
    /// `name`: rows of one page that all sit in the same file (a dead-code
    /// page of one module, a callers page) each keep it, so no row has to be
    /// read against a trailing `shared` map to know which file it is in.
    #[test]
    fn a_rows_file_is_never_hoisted() {
        let mut rows = vec![json!({"index":0,"data":{"nodes":[
            {"file":"a.ts","name":"one","distance":1},
            {"file":"a.ts","name":"two","distance":1}
        ]}})];
        let shared = super::hoist(ToolId::StructureSearch, &mut rows)
            .expect("an identical non-identity field still hoists");
        assert!(!shared.contains_key("file"), "{shared:?}");
        assert_eq!(shared["distance"], 1);
        assert_eq!(rows[0]["data"]["nodes"][0]["file"], "a.ts");
        assert_eq!(rows[0]["data"]["nodes"][1]["file"], "a.ts");
    }

    /// The checked-out head is a row field stated once in `shared`; it never
    /// belongs on nested entries such as localFetch `blocks`.
    #[test]
    fn local_fetch_blocks_validate_with_a_shared_head() {
        let output = json!({"shared":{"commitSha":"abc123"},"results":[{"index":0,"data":{
            "path":"src/a.ts","content":"1\tfunction a() {}\n","totalLines":1,
            "blocks":[{"symbolName":"a","line":1,"endLine":1}]
        }}]});
        validate_output("localFetch", &output).expect("shared head validates on row data");
    }

    /// One representative response per tool, with the nested arrays each
    /// tool emits (localFetch/ghGetFileContent `blocks`, lspSearch
    /// `builtinLib`, ghStructure ref maps, gh rows `lines`, history rows),
    /// compacted as the response stage does (entry hoist, plus the shared
    /// checked-out head on local reads), validates after restore.
    #[test]
    fn compacted_representative_responses_validate_for_every_tool() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let block = json!([{"symbolName":"a","line":1,"endLine":3},{"symbolName":"b","line":5,"endLine":9}]);
        let cases = [
            (
                "localSearch",
                true,
                json!({"files":[
                    {"path":"a.rs","matches":[{"line":3,"value":"fn restore()"}]},
                    {"path":"b.rs","matches":[{"line":4,"value":"fn restore()"}]}
                ]}),
            ),
            (
                "localFetch",
                true,
                json!({"path":"a.ts","content":"1\tx\n","totalLines":9,"blocks":block}),
            ),
            (
                "structureSearch",
                true,
                json!({"path":"src","files":[{"dir":"src","files":["a.rs (1)"]},{"dir":"src/d","files":["b.rs (2)"]}]}),
            ),
            (
                "lspSearch",
                false,
                json!({"path":"a.ts","kind":"callees","files":[
                {"path":"a.ts","matches":[{"symbolName":"b","kind":"function","line":4,"endLine":6,"sites":[{"line":2,"column":1}]}]}
            ],"builtinLib":["map","filter"],
                "hints":{"findSimilar":{"tool":"lspSearch","query":{"queries":[{"path":"a.ts","symbolName":"a","line":2,"operation":"callees"}]}}}}),
            ),
            (
                "astSearch",
                false,
                json!({"files":[
                    {"path":"a.ts","matches":[{"line":1,"column":1,"endLine":2,"value":"f(x)"}]},{"path":"b.ts","matches":[{"line":3,"column":1,"endLine":4,"value":"f(y)"}]}
                ]}),
            ),
            (
                "ghSearchCode",
                false,
                json!({"files":[
                    {"path":"a.js","lines":["5\tvar a = 1;"],"commitSha":sha},
                    {"path":"b.js","lines":["6\tvar b = 2;"],"commitSha":sha}
                ]}),
            ),
            (
                "ghGetFileContent",
                false,
                json!({"path":"lib/a.js","content":"1\tx\n","totalLines":9,"commitSha":sha,"blocks":block}),
            ),
            (
                "ghStructure",
                false,
                json!({"defaultBranch":"main","commitSha":sha,"branches":{"main":sha,"dev":sha},"tags":["v1"]}),
            ),
            (
                "ghStructure",
                false,
                json!({"files":[{"dir":"lib","files":["a.js","b.js"]},{"dir":"lib/x","files":["c.js"]}],"commitSha":sha}),
            ),
            (
                "ghSearchHistory",
                false,
                json!({"commits":[
                    {"commitSha":sha,"date":"2026-10-06T21:16:14Z","messageHeadline":"a (#1)","author":"x"},
                    {"commitSha":"abc","date":"2026-10-06T21:16:14Z","messageHeadline":"b (#2)","author":"x"}
                ]}),
            ),
            (
                "ghGetHistoryItem",
                false,
                json!({"pullRequests":[{"number":1,"title":"t","state":"merged","author":"x","createdAt":"2026-10-06T21:16:14Z",
                "baseBranch":"main","headBranch":"f","headSha":sha,"baseSha":sha,"mergeCommitSha":sha,
                "commitsCount":1,"changedFilesCount":2,"additions":2,"deletions":2}]}),
            ),
            (
                "ghSearchRepo",
                false,
                json!({"repositories":[
                    {"owner":"o","repo":"a","stars":1,"language":"JavaScript","topics":["x"]},
                    {"owner":"o","repo":"b","stars":2,"language":"JavaScript","topics":["y"]}
                ]}),
            ),
        ];
        for (tool, local, data) in cases {
            let rows = vec![
                json!({"index":0,"data":data.clone()}),
                json!({"index":1,"data":data}),
            ];
            let id = ToolId::from_name(tool).expect("known tool");
            let mut output = crate::response::rows::envelope(id, rows);
            if local {
                output["shared"]["commitSha"] = json!(sha);
            }
            crate::response::channels::publish_hints(&mut output, id);
            if let Err(error) = validate_output(tool, &output) {
                panic!("{tool}: {:?}\n{output}", error.issues);
            }
        }
    }

    /// A field an entry schema requires stays on every entry, so the wire a
    /// client receives validates as returned: two diagnostics sharing their
    /// `message` keep it.
    #[test]
    fn required_entry_fields_stay_on_the_wire() {
        let original = json!({"results":[{"index":0,"data":{"kind":"diagnostics","matches":[
            {"message":"unused","line":1},
            {"message":"unused","line":2}
        ]}}]});
        validate_output("lspSearch", &original).expect("the original row is valid");
        let mut rows = original["results"].as_array().expect("rows").clone();
        let shared = super::hoist(ToolId::LspSearch, &mut rows);
        assert!(
            shared
                .as_ref()
                .is_none_or(|shared| !shared.contains_key("message")),
            "{shared:?}"
        );
        let mut wire = json!({"results": rows});
        if let Some(shared) = shared {
            wire["shared"] = serde_json::Value::Object(shared);
        }
        validate_output("lspSearch", &wire).expect("the returned wire validates");
        for entry in wire["results"][0]["data"]["matches"]
            .as_array()
            .expect("matches")
        {
            assert_eq!(entry["message"], "unused", "{wire}");
        }
    }

    /// Validation checks exactly the JSON a client receives: a `shared` map
    /// never stands in for a required entry field.
    #[test]
    fn validation_never_restores_shared_fields() {
        let wire = json!({"shared":{"message":"unused"},"results":[{"index":0,"data":{
            "kind":"diagnostics","matches":[{"line":1},{"line":2}]
        }}]});
        assert!(validate_output("lspSearch", &wire).is_err(), "{wire}");
    }

    /// Hoisting a shared key leaves every entry's other keys in the order
    /// the tool wrote them.
    #[test]
    fn hoisting_keeps_the_entry_key_order() {
        let mut rows = vec![json!({"index":0,"data":{"rows":[
            {"a":1,"same":"x","b":2,"c":3},
            {"a":4,"same":"x","b":5,"c":6}
        ]}})];
        let shared =
            super::hoist(ToolId::StructureSearch, &mut rows).expect("the shared key hoists");
        assert_eq!(shared["same"], "x");
        for row in rows[0]["data"]["rows"].as_array().expect("rows") {
            let keys = row
                .as_object()
                .expect("entry")
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>();
            assert_eq!(keys, ["a", "b", "c"], "{row}");
        }
    }

    /// Outline rows keep their names even when every one shares it (a
    /// struct and two `impl Foo` blocks).
    #[test]
    fn outline_container_names_stay_on_their_rows() {
        let mut rows = vec![json!({"index":0,"data":{"symbols":[
            {"symbolName":"Foo","kind":"struct","line":1},
            {"symbolName":"Foo","kind":"impl","line":3,"endLine":5,"members":[{"symbolName":"a","kind":"function","line":4}]},
            {"symbolName":"Foo","kind":"impl","line":7,"endLine":9,"members":[{"symbolName":"b","kind":"function","line":8}]}
        ]}})];
        assert!(
            super::hoist(ToolId::AstSearch, &mut rows).is_none(),
            "{rows:?}"
        );
        for row in rows[0]["data"]["symbols"].as_array().expect("rows") {
            assert_eq!(row["symbolName"], "Foo", "{rows:?}");
        }
    }

    /// The guard is per tool: a field another tool's entries require still
    /// hoists from this tool's entries, and the tool that requires it keeps
    /// it on every entry.
    #[test]
    fn the_required_field_guard_is_the_responding_tools() {
        let required = |tool| super::required_entry_fields(tool).contains("value");
        assert!(required(ToolId::LspSearch) && !required(ToolId::StructureSearch));
        let rows = || {
            vec![json!({"index":0,"data":{"nodes":[
                {"file":"a.ts","value":"same"},
                {"file":"b.ts","value":"same"}
            ]}})]
        };
        let mut hoisted = rows();
        let shared = super::hoist(ToolId::StructureSearch, &mut hoisted).expect("value hoists");
        assert_eq!(shared["value"], "same");
        assert!(hoisted[0]["data"]["nodes"][0].get("value").is_none());
        let mut kept = rows();
        assert!(
            super::hoist(ToolId::LspSearch, &mut kept).is_none(),
            "{kept:?}"
        );
        assert_eq!(kept[0]["data"]["nodes"][1]["value"], "same");
    }
}
