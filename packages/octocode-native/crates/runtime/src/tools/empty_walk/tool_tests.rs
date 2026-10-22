//! One scenario table, run against every local walking tool: when a walk is
//! empty because its matches sit behind a skipped path, each tool names the
//! rule or directory, and its lead (`next.includeSkipped`) lifts exactly the
//! causes that held matches and runs to the match. When nothing sits behind
//! the skipped paths there is no lead.
use super::Cause;
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, simple_policy};
use serde_json::{Value, json};
use std::path::Path;

const FLAGGED: [Cause; 3] = [Cause::Ignored, Cause::Hidden, Cause::DefaultExcluded];

/// A directory the walk skips, holding `needle.ts` (or, in the empty
/// scenario, a file nothing searches for).
struct Scenario {
    name: &'static str,
    gitignore: &'static str,
    /// Where `needle.ts` sits, root-relative.
    dir: &'static str,
    /// The causes that skip it.
    causes: &'static [Cause],
    /// How the diagnosis names each cause.
    reasons: &'static [(Cause, &'static str)],
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "gitignored",
        gitignore: "gen/\n",
        dir: "gen/inner",
        causes: &[Cause::Ignored],
        reasons: &[(Cause::Ignored, ".gitignore:1 'gen/'")],
    },
    Scenario {
        name: "hidden",
        gitignore: "",
        dir: ".cfg/inner",
        causes: &[Cause::Hidden],
        reasons: &[(Cause::Hidden, "hidden .cfg/")],
    },
    Scenario {
        name: "default-excluded",
        gitignore: "",
        dir: "node_modules/pkg",
        causes: &[Cause::DefaultExcluded],
        reasons: &[(Cause::DefaultExcluded, "node_modules/")],
    },
    Scenario {
        name: "gitignored and hidden",
        gitignore: ".gen/\n",
        dir: ".gen",
        causes: &[Cause::Ignored, Cause::Hidden],
        reasons: &[
            (Cause::Ignored, ".gitignore:1 '.gen/'"),
            (Cause::Hidden, "hidden .gen/"),
        ],
    },
    Scenario {
        name: "gitignored and default-excluded",
        gitignore: "build/\n",
        dir: "build/out",
        causes: &[Cause::Ignored, Cause::DefaultExcluded],
        reasons: &[
            (Cause::Ignored, ".gitignore:1 'build/'"),
            (Cause::DefaultExcluded, "build/"),
        ],
    },
];

/// One walking tool, driven through its own public entry.
struct Tool {
    name: &'static str,
    /// Causes the tool's default walk skips by.
    skips: &'static [Cause],
    /// The `next` channel that carries the lead for `scenario`.
    channel: fn(&Scenario) -> &'static str,
    /// The query row over `root` that looks for `needle`.
    query: fn(&Path) -> Value,
    run: fn(&Path, &Value) -> Value,
    /// The run found the needle.
    found: fn(&Value) -> bool,
    /// How the tool's lead lifts `cause` for `scenario`, if it does.
    lifts: fn(&Value, Cause, &Path, &Scenario) -> bool,
    /// An empty run says something in prose even with nothing behind the
    /// skipped paths.
    prose_when_nothing_found: bool,
}

/// A lead that spells walk flags as fields lifts `cause` by setting its flag.
fn by_flag(lead: &Value, cause: Cause, _: &Path, _: &Scenario) -> bool {
    let (field, value) = cause.flag();
    lead.get(field) == Some(&value)
}

fn row(root: &Path, fields: Value) -> Value {
    let mut row = json!({"path": root, "mainGoal": "test", "reasoning": "test"});
    row.as_object_mut()
        .expect("object")
        .extend(fields.as_object().expect("fields").clone());
    row
}

fn non_empty(out: &Value, field: &str) -> bool {
    out[field].as_array().is_some_and(|rows| !rows.is_empty())
}

fn local_search() -> Tool {
    use crate::tools::local_search::{LocalSearchQuery, execute_local_search};
    Tool {
        name: "localSearch",
        skips: &FLAGGED,
        channel: |_| "includeSkipped",
        query: |root| row(root, json!({"matchString": "needle"})),
        run: |root, query| {
            let (paths, security) = simple_policy(root);
            let query: LocalSearchQuery = serde_json::from_value(query.clone()).expect("query");
            let result =
                execute_local_search(&query, &paths, &security, &NeverCancel, None, None, None)
                    .expect("search");
            serde_json::to_value(result).expect("result")
        },
        found: |out| non_empty(out, "files"),
        lifts: by_flag,
        prose_when_nothing_found: true,
    }
}

fn ast_match() -> Tool {
    Tool {
        name: "astSearch match",
        skips: &FLAGGED,
        channel: |_| "includeSkipped",
        query: |root| {
            row(
                root,
                json!({"operation": "match", "language": "typescript", "pattern": "function needle() {}"}),
            )
        },
        run: |root, query| run_ast(root, query),
        found: |out| out["matchCount"].as_u64().is_some_and(|count| count > 0),
        lifts: by_flag,
        prose_when_nothing_found: true,
    }
}

fn run_ast(root: &Path, query: &Value) -> Value {
    use crate::tools::ast_search::{AstSearchQuery, execute_ast};
    let (paths, security) = simple_policy(root);
    let query: AstSearchQuery = serde_json::from_value(query.clone()).expect("query");
    execute_ast(&query, &paths, &security, &NeverCancel, None).expect("astSearch")
}

/// `symbols` honors `.gitignore` and hides dot entries like `match`;
/// `includeIgnored`, `includeHidden` and `defaultExcludes:false` lift them.
fn ast_symbols() -> Tool {
    Tool {
        name: "astSearch symbols",
        skips: &[Cause::Ignored, Cause::Hidden, Cause::DefaultExcluded],
        channel: |_| "includeSkipped",
        query: |root| {
            row(
                root,
                json!({"operation": "symbols", "symbolName": "needle"}),
            )
        },
        run: |root, query| run_ast(root, query),
        found: |out| non_empty(out, "files"),
        lifts: by_flag,
        prose_when_nothing_found: true,
    }
}

fn structure(operation: &'static str, skips: &'static [Cause]) -> Tool {
    Tool {
        name: if operation == "tree" {
            "structureSearch tree"
        } else {
            "structureSearch files"
        },
        skips,
        channel: |_| "includeSkipped",
        query: if operation == "tree" {
            |root| row(root, json!({"operation": "tree", "extensions": ["ts"]}))
        } else {
            |root| row(root, json!({"operation": "files", "extensions": ["ts"]}))
        },
        run: |root, query| {
            use crate::tools::structure_search::{StructureSearchQuery, execute_structure};
            let (paths, security) = simple_policy(root);
            let query: StructureSearchQuery = serde_json::from_value(query.clone()).expect("query");
            execute_structure(&query, &paths, &security, &NeverCancel, None).expect("structure")
        },
        found: |out| non_empty(out, "files"),
        lifts: by_flag,
        prose_when_nothing_found: false,
    }
}

fn tools() -> Vec<Tool> {
    vec![
        local_search(),
        ast_match(),
        ast_symbols(),
        structure("tree", &FLAGGED),
        structure("files", &FLAGGED),
    ]
}

/// What a result says in prose (leads excluded): every hint, in order, and
/// every warning.
fn prose(out: &Value) -> String {
    let mut text = Vec::new();
    let hints = match &out["hints"] {
        Value::Object(map) => map.get("text"),
        hints => Some(hints),
    };
    if let Some(Value::Array(items)) = hints {
        text.extend(items.iter().filter_map(Value::as_str));
    }
    if let Value::Array(items) = &out["warnings"] {
        text.extend(items.iter().filter_map(Value::as_str));
    }
    text.join(" | ")
}

fn lead_query(out: &Value, channel: &str) -> Option<Value> {
    out["next"].get(channel)?["query"]["queries"]
        .get(0)
        .cloned()
}

/// `root` with a visible markdown file and the scenario's skipped directory
/// holding `file`; a git marker and `.gitignore` when the scenario has
/// ignore rules. Without rules it lies outside any git work tree, where a
/// default-excluded name is pruned by its name alone (inside one, only
/// where git ignores it).
fn repo(scenario: &Scenario, file: &str, text: &str) -> Fixture {
    let root = Fixture::new();
    if !scenario.gitignore.is_empty() {
        std::fs::create_dir(root.0.join(".git")).expect("repository marker");
        std::fs::write(root.0.join(".gitignore"), scenario.gitignore).expect("gitignore");
    }
    std::fs::write(root.0.join("visible.md"), "nothing here\n").expect("visible");
    std::fs::create_dir_all(root.0.join(scenario.dir)).expect("skipped dir");
    std::fs::write(root.0.join(scenario.dir).join(file), text).expect("skipped file");
    root
}

fn describe(tool: &Tool, scenario: &Scenario) -> String {
    format!("{} / {}", tool.name, scenario.name)
}

#[test]
fn a_match_behind_a_skipped_path_is_named_and_its_lead_lifts_only_that() {
    for tool in tools() {
        for scenario in SCENARIOS {
            let label = describe(&tool, scenario);
            let root = repo(scenario, "needle.ts", "function needle() {}\n");
            let query = (tool.query)(&root.0);
            let out = (tool.run)(&root.0, &query);
            let effective = FLAGGED
                .into_iter()
                .filter(|cause| scenario.causes.contains(cause) && tool.skips.contains(cause))
                .collect::<Vec<_>>();
            if effective.is_empty() {
                assert!((tool.found)(&out), "{label}: walked without a lead: {out}");
                continue;
            }
            assert!(
                !(tool.found)(&out),
                "{label}: found without the lead: {out}"
            );
            let lead = lead_query(&out, (tool.channel)(scenario))
                .unwrap_or_else(|| panic!("{label}: no lead: {out}"));
            // Validates against the contract.
            crate::contracts::validate(
                if tool.name.starts_with("astSearch") {
                    "astSearch"
                } else if tool.name.starts_with("structure") {
                    "structureSearch"
                } else {
                    tool.name
                },
                json!({"queries": [lead.clone()]}),
            )
            .unwrap_or_else(|error| panic!("{label}: lead invalid {lead}: {:?}", error.issues));
            for cause in FLAGGED
                .into_iter()
                .filter(|cause| tool.skips.contains(cause))
            {
                assert_eq!(
                    (tool.lifts)(&lead, cause, &root.0, scenario),
                    effective.contains(&cause),
                    "{label}: lead should lift exactly {effective:?}: {lead}"
                );
            }
            // The hint names the deciding rule, entry or directory.
            let first = effective[0];
            let reason = scenario
                .reasons
                .iter()
                .find(|(cause, _)| *cause == first)
                .map(|(_, reason)| *reason)
                .expect("scenario names the cause");
            if tool.name == "localSearch" {
                let entry_path = scenario.dir.split('/').next().expect("directory");
                assert!(
                    out["skippedPaths"]["entries"]
                        .as_array()
                        .expect("skipped entries")
                        .iter()
                        .any(|entry| entry["path"]
                            == json!(
                                root.0
                                    .canonicalize()
                                    .expect("canonical fixture")
                                    .join(entry_path)
                            )
                            && entry["cause"] == first.label()
                            && entry["reason"] == reason),
                    "{label}: exact skipped path, cause and `{reason}` in {out}"
                );
            } else {
                assert!(prose(&out).contains(reason), "{label}: `{reason}` in {out}");
            }
            // The lead runs and finds the match.
            let found = (tool.run)(&root.0, &lead);
            assert!((tool.found)(&found), "{label}: lead found nothing: {found}");
        }
    }
}

#[test]
fn nothing_behind_the_skipped_paths_leads_nowhere() {
    for tool in tools() {
        for scenario in SCENARIOS {
            let label = describe(&tool, scenario);
            let root = repo(scenario, "other.md", "nothing to find\n");
            let query = (tool.query)(&root.0);
            let out = (tool.run)(&root.0, &query);
            assert!(!(tool.found)(&out), "{label}: {out}");
            for channel in ["includeSkipped", "narrowScope"] {
                assert!(
                    lead_query(&out, channel).is_none(),
                    "{label}: lead to nothing: {out}"
                );
            }
            if tool.prose_when_nothing_found {
                assert!(!prose(&out).is_empty(), "{label}: no fallback hint: {out}");
            }
        }
    }
}
