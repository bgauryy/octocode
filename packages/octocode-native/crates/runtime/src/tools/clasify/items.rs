//! Scout over list results. A list page whose items carry content (matches,
//! declarations, references, repositories, pull requests, issues, commits,
//! packages) is judged one item at a time, and every item carries the read
//! that fetches it, so the host reads only the items the judge keeps. Bare
//! path lists (file listings, trees) stay one page: a name alone is judged
//! better comparatively, with a `choice` over the page. Text search keeps its
//! own candidate path (`run::hydrate`) because it can hydrate file chunks.
//!
//! Each list tool names its own items through
//! [`ToolOutput::clasify_items`](crate::tools::output::ToolOutput::clasify_items);
//! this module holds what those hooks share and the symbols-outline paging.
use super::resource::ResourceSource;
use crate::tools::id::ToolId;
use serde_json::{Value, json};

/// One candidate: the narrowed state the provider judges, its identity, and
/// the executable read that fetches it.
pub struct Item {
    pub state: Value,
    pub read: Option<Value>,
    /// Absolute local path or `owner/repo/path` for file candidates.
    pub path: Option<String>,
    /// Identity of a non-file candidate (`owner/repo#12`, `npm:ajv`).
    pub item: Option<String>,
}

/// Split one captured list page into candidates. `None` leaves the page whole:
/// the tool has no candidate list, or the page lists nothing.
pub(crate) fn split(source: &ResourceSource, state: &Value) -> Option<Vec<Item>> {
    let items = source.tool().output().clasify_items(source, state)?;
    (!items.is_empty()).then_some(items)
}

/// The list page's row data, which every hook narrows.
pub(crate) fn page_data(state: &Value) -> Option<&Value> {
    state.pointer("/results/0/data")
}

/// The page's shared path `root`, which relative row paths join.
pub(crate) fn page_root(state: &Value) -> Option<&str> {
    state.get("root").and_then(Value::as_str)
}

/// A directory symbols outline page: declaration rows grouped per file under
/// `files`. The outline pages by rows, so one file can straddle pages.
pub(crate) fn is_symbol_outline(source: &ResourceSource, state: &Value) -> bool {
    source.is_symbols()
        && state
            .pointer("/results/0/data/files")
            .is_some_and(Value::is_array)
}

fn outline_files(state: &Value) -> &[Value] {
    state
        .pointer("/results/0/data/files")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Absolute path of an outline file; page `root`s can differ between pages.
fn outline_path(state: &Value, file: &Value) -> Option<String> {
    let base = state.get("root").and_then(Value::as_str);
    Some(absolute(base, file.get("path")?.as_str()?))
}

pub(super) fn first_outline_path(state: &Value) -> Option<String> {
    outline_path(state, outline_files(state).first()?)
}

pub(super) fn last_outline_path(state: &Value) -> Option<String> {
    outline_files(state)
        .last()
        .and_then(|file| outline_path(state, file))
}

/// Drop the page's first file (its rows were judged with the page that
/// started it); a page's only file is kept.
pub(super) fn drop_first_outline_file(state: &mut Value) {
    if let Some(files) = state
        .pointer_mut("/results/0/data/files")
        .and_then(Value::as_array_mut)
        .filter(|files| files.len() > 1)
    {
        files.remove(0);
    }
}

/// Append the declarations `following` holds for `state`'s last file.
/// `None`: `following` starts with another file. `Some(more)`: rows were
/// appended, and `more` says whether `following` also lists other files.
pub(super) fn extend_last_outline_file(state: &mut Value, following: &Value) -> Option<bool> {
    let files = outline_files(following);
    let first = files.first()?;
    if outline_path(following, first)? != last_outline_path(state)? {
        return None;
    }
    let rows = first.get("symbols")?.as_array()?.clone();
    state
        .pointer_mut("/results/0/data/files")?
        .as_array_mut()?
        .last_mut()?
        .get_mut("symbols")?
        .as_array_mut()?
        .extend(rows);
    Some(files.len() > 1)
}

pub(crate) fn narrowed(state: &Value, pointer: &str, items: Vec<Value>) -> Value {
    let mut narrowed = state.clone();
    if let Some(slot) = narrowed.pointer_mut(pointer) {
        *slot = Value::Array(items);
    }
    narrowed
}

pub(crate) fn absolute(base: Option<&str>, path: &str) -> String {
    match base.filter(|_| !std::path::Path::new(path).is_absolute()) {
        Some(base) => std::path::Path::new(base)
            .join(path)
            .to_string_lossy()
            .into_owned(),
        None => path.to_owned(),
    }
}

/// A kept candidate's fetch. It serves the scouting matrix, whose brief the
/// response stage copies onto it (see `continuations::inherit_clasify_briefs`).
pub(crate) fn read(tool: ToolId, query: serde_json::Map<String, Value>) -> Value {
    json!({"tool":tool.as_str(),"confidence":"high","query":query})
}

/// A local read around the densest run of anchor lines, or the whole file
/// when the item carries no line.
pub(crate) fn local_read(path: &str, lines: Vec<u64>) -> Value {
    let mut query = serde_json::Map::new();
    query.insert("path".into(), json!(path));
    if let Some(center) = super::run::densest_match_line(lines) {
        let radius = super::run::HYDRATED_LINE_RADIUS;
        let start = center.saturating_sub(radius).max(1);
        let end = center.saturating_add(radius);
        query.insert("ranges".into(), json!([format!("{start}-{end}")]));
    }
    read(ToolId::LocalFetch, query)
}

/// The 1-based line a result row names: a bare number (`lines` lists), the
/// leading number of a compact string row (symbols outline `"<line>[-<end>]
/// kind name"`, structural match `"<line>[-<end>]\t<value>"`, reference
/// `"<line>:<col> <text>"`, caller `"<line>:<col> in …"`), or an object
/// row's `key`. Every row reader here goes through this accessor, so either
/// row shape yields the same candidate lines.
pub(crate) fn line(value: &Value, key: &str) -> Option<u64> {
    if let Some(line) = value.as_u64() {
        return Some(line);
    }
    if let Some(text) = value.as_str() {
        let text = text.trim_start();
        let end = text
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len());
        return text[..end].parse().ok();
    }
    value.get(key).and_then(Value::as_u64)
}

/// Per-file rows (astSearch matches, a directory outline's symbols): one
/// candidate per file, reading the lines its `rows` name.
pub(crate) fn file_items(
    state: &Value,
    data: &Value,
    base: Option<&str>,
    rows: &str,
) -> Option<Vec<Item>> {
    let items = data
        .get("files")?
        .as_array()?
        .iter()
        .filter_map(|file| {
            let path = absolute(base, file.get("path")?.as_str()?);
            let lines = file
                .get(rows)
                .and_then(Value::as_array)
                .map(|rows| rows.iter().filter_map(|row| line(row, "line")).collect())
                .unwrap_or_default();
            Some(Item {
                state: narrowed(state, "/results/0/data/files", vec![file.clone()]),
                read: Some(local_read(&path, lines)),
                path: Some(path),
                item: None,
            })
        })
        .collect();
    Some(items)
}

/// Group rows that name their file under `key` (declarations, locations) by
/// path, keeping first-seen order. Rows without a path belong to `fallback`.
pub(crate) fn grouped<'a>(
    rows: &'a [Value],
    fallback: Option<&'a str>,
) -> Vec<(&'a str, Vec<Value>)> {
    let mut groups: Vec<(&str, Vec<Value>)> = Vec::new();
    for row in rows {
        let Some(path) = row.get("path").and_then(Value::as_str).or(fallback) else {
            continue;
        };
        match groups.iter_mut().find(|(seen, _)| *seen == path) {
            Some((_, group)) => group.push(row.clone()),
            None => groups.push((path, vec![row.clone()])),
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Split a page read by the `{tool, query}` source.
    fn split(source: &Value, state: &Value) -> Option<Vec<Item>> {
        super::split(&ResourceSource::of(source)?, state)
    }

    fn is_symbol_outline(source: &Value, state: &Value) -> bool {
        ResourceSource::of(source).is_some_and(|read| super::is_symbol_outline(&read, state))
    }

    /// First line of an item read's `ranges` window.
    fn first_line(item: &Item) -> u64 {
        let query = &item.read.as_ref().expect("read")["query"];
        let window = query["ranges"][0].as_str().expect("ranges");
        window
            .split_once('-')
            .expect("a-b")
            .0
            .parse()
            .expect("line")
    }

    fn wrap(data: Value) -> Value {
        json!({"root":"/repo","results":[{"data":data}]})
    }

    #[test]
    fn ast_matches_split_per_file_with_a_follow_up_read() {
        let source = json!({"tool":"astSearch","query":{"operation":"match","pattern":"$X","path":"/repo","mainGoal":"g"}});
        let state = wrap(json!({"files":[
            {"path":"a.rs","matches":[{"line":40}]},{"path":"/abs/c.rs","matches":[]}
        ]}));
        let items = split(&source, &state).expect("match split");
        let paths: Vec<_> = items.iter().filter_map(|item| item.path.clone()).collect();
        assert_eq!(paths, ["/repo/a.rs", "/abs/c.rs"]);
        let read = items[0].read.as_ref().unwrap();
        assert_eq!(read["tool"], "localFetch");
        assert_eq!(read["query"]["ranges"], json!(["1-100"]));
        assert!(
            read["query"].get("mainGoal").is_none(),
            "the matrix brief is added later"
        );
        assert!(
            items[1].read.as_ref().unwrap()["query"]
                .get("ranges")
                .is_none()
        );
        assert_eq!(
            items[0].state["results"][0]["data"]["files"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn outline_pages_join_a_file_split_across_pages() {
        let page = |files: Value| json!({"root":"/r","results":[{"data":{"files":files}}]});
        let source = json!({"tool":"astSearch","query":{"operation":"symbols","path":"/r"}});
        let mut first = page(json!([
            {"path":"a.rs","symbols":[{"name":"a1","line":1}]},
            {"path":"b.rs","symbols":[{"name":"b1","line":1}]}
        ]));
        assert!(is_symbol_outline(&source, &first));
        let only_b = page(json!([{"path":"b.rs","symbols":[{"name":"b2","line":2}]}]));
        assert_eq!(extend_last_outline_file(&mut first, &only_b), Some(false));
        let b_then_c = json!({"root":"/","results":[{"data":{"files":[
            {"path":"r/b.rs","symbols":[{"name":"b3","line":3}]},
            {"path":"r/c.rs","symbols":[{"name":"c1","line":1}]}
        ]}}]});
        assert_eq!(extend_last_outline_file(&mut first, &b_then_c), Some(true));
        assert_eq!(
            first["results"][0]["data"]["files"][1]["symbols"]
                .as_array()
                .map(Vec::len),
            Some(3)
        );
        let c_only = page(json!([{"path":"c.rs","symbols":[{"name":"c1","line":1}]}]));
        assert_eq!(extend_last_outline_file(&mut first, &c_only), None);

        let mut resumed = b_then_c.clone();
        assert_eq!(first_outline_path(&resumed).as_deref(), Some("/r/b.rs"));
        assert_eq!(last_outline_path(&first).as_deref(), Some("/r/b.rs"));
        drop_first_outline_file(&mut resumed);
        assert_eq!(first_outline_path(&resumed).as_deref(), Some("/r/c.rs"));
        drop_first_outline_file(&mut resumed);
        assert_eq!(first_outline_path(&resumed).as_deref(), Some("/r/c.rs"));
    }

    #[test]
    fn declarations_and_references_group_by_file_with_line_windows() {
        let symbols = json!({"tool":"astSearch","query":{"operation":"symbols","path":"/repo"}});
        let state = wrap(json!({"symbols":[
            {"name":"a","line":10,"path":"x.rs"},{"name":"b","line":300,"path":"y.rs"},{"name":"c","line":20,"path":"x.rs"}
        ]}));
        let items = split(&symbols, &state).expect("symbols split");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].path.as_deref(), Some("/repo/x.rs"));
        assert_eq!(
            items[0].state["results"][0]["data"]["symbols"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(first_line(&items[1]), 240);

        let grouped = wrap(json!({"files":[
            {"path":"x.rs","symbols":[{"name":"a","line":10},{"name":"c","line":20}]},
            {"path":"y.rs","symbols":[{"name":"b","line":300}]}
        ]}));
        let items = split(&symbols, &grouped).expect("grouped symbols split");
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].path.as_deref(), Some("/repo/y.rs"));
        assert_eq!(first_line(&items[1]), 240);

        single_outline(json!([{"name":"a","line":1}]));

        let refs = json!({"tool":"lspSearch","query":{"operation":"references","path":"/repo/a.rs","symbolName":"a","lineHint":1}});
        let state = wrap(json!({"payload":{"matches":[
            {"displayRange":{"startLine":5},"path":"a.rs"},{"displayRange":{"startLine":9},"path":"b.rs"}
        ]}}));
        let items = split(&refs, &state).expect("refs split");
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].path.as_deref(), Some("/repo/b.rs"));
        let hover = wrap(json!({"payload":{"kind":"hover"}}));
        assert!(split(&refs, &hover).is_none());
    }

    /// Split a single-file outline of `/repo/one.rs` holding `symbols`; its
    /// one candidate names that file.
    fn single_outline(symbols: Value) -> Vec<Item> {
        let single =
            json!({"tool":"astSearch","query":{"operation":"symbols","path":"/repo/one.rs"}});
        let state = json!({"results":[{"data":{"path":"/repo/one.rs","symbols":symbols}}]});
        let items = split(&single, &state).expect("single outline");
        assert_eq!(items[0].path.as_deref(), Some("/repo/one.rs"));
        items
    }

    #[test]
    fn compact_string_rows_yield_the_same_candidates_as_object_rows() {
        // Outline rows (directory and single file), lean match rows, compact
        // references and direct callers all name their line first.
        let symbols = json!({"tool":"astSearch","query":{"operation":"symbols","path":"/repo"}});
        let grouped = wrap(json!({"files":[
            {"path":"x.rs","symbols":["10-12 struct A +","  11 function a"]},
            {"path":"y.rs","symbols":["300 function b doc"]}
        ]}));
        let items = split(&symbols, &grouped).expect("grouped outline split");
        assert_eq!(items.len(), 2);
        assert_eq!(first_line(&items[0]), 1);
        assert_eq!(first_line(&items[1]), 240);
        let items = single_outline(json!(["400 function a"]));
        assert_eq!(first_line(&items[0]), 340);

        let matches =
            json!({"tool":"astSearch","query":{"operation":"match","pattern":"$X","path":"/repo"}});
        let state = wrap(
            json!({"files":[{"path":"a.rs","matches":["400\tx.unwrap()","402-410\tfn f() …"]}]}),
        );
        let items = split(&matches, &state).expect("lean match split");
        // Centered between lines 400 and 402.
        assert_eq!(first_line(&items[0]), 341);

        let refs = json!({"tool":"lspSearch","query":{"operation":"references","path":"/repo/a.rs","symbolName":"a","lineHint":1}});
        let state = wrap(json!({"payload":{"kind":"references","files":[
            {"path":"a.rs","matches":["281:20 fn try_read_output("]},
            {"path":"b.rs","matches":["368:14 harness.try_read_output(out, waker);"]}
        ]}}));
        let items = split(&refs, &state).expect("compact refs split");
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].path.as_deref(), Some("/repo/b.rs"));
        assert_eq!(first_line(&items[1]), 308);
        assert_eq!(
            items[1].state["results"][0]["data"]["payload"]["files"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        let callers = json!({"tool":"lspSearch","query":{"operation":"callers","path":"/repo/a.rs","symbolName":"a","lineHint":1}});
        let state = wrap(json!({"payload":{"kind":"callers","files":[
            {"path":"c.ts","matches":["359:38 in function render 97-848"]}
        ]}}));
        let items = split(&callers, &state).expect("callers split");
        assert_eq!(items[0].path.as_deref(), Some("/repo/c.ts"));
        assert_eq!(first_line(&items[0]), 299);
    }

    #[test]
    fn remote_lists_carry_item_identity_and_fetch_reads() {
        let repos = json!({"tool":"ghSearchRepo","query":{"keywords":["mcp"]}});
        let state =
            json!({"results":[{"data":{"repositories":[{"owner":"o","repo":"r","stars":1}]}}]});
        let items = split(&repos, &state).unwrap();
        assert_eq!(items[0].item.as_deref(), Some("o/r"));
        assert_eq!(items[0].read.as_ref().unwrap()["tool"], "ghStructure");

        let prs = json!({"tool":"ghSearchHistory","query":{"operation":"pullRequest","owner":"o","repo":"r"}});
        let state = json!({"results":[{"data":{"pullRequests":[{"number":7,"title":"t"},{"number":8,"repository":"x/y"}]}}]});
        let items = split(&prs, &state).unwrap();
        assert_eq!(items[0].item.as_deref(), Some("o/r#7"));
        assert_eq!(
            items[1].read.as_ref().unwrap()["query"],
            json!({"operation":"pullRequest","number":8,"owner":"x","repo":"y"})
        );

        let commits =
            json!({"tool":"ghSearchHistory","query":{"operation":"commit","owner":"o","repo":"r"}});
        let state = json!({"results":[{"data":{"commits":[{"sha":"abcdef0123456789"},{"message":"no sha"}]}}]});
        let items = split(&commits, &state).unwrap();
        assert_eq!(items[0].item.as_deref(), Some("o/r@abcdef012345"));
        assert_eq!(
            items[0].read.as_ref().unwrap()["query"]["ref"],
            "abcdef0123456789"
        );
        assert!(items[1].read.is_none() && items[1].item.is_none());

        // Pull requests may be searched across GitHub; issues and commits
        // name their repository.
        let orgwide =
            json!({"tool":"ghSearchHistory","query":{"operation":"pullRequest","keywords":["x"]}});
        let state = json!({"results":[{"data":{"pullRequests":[{"number":1}]}}]});
        assert!(
            split(&orgwide, &state).unwrap()[0].read.is_none(),
            "no repository: no invented read"
        );

        let packages = json!({"tool":"artifactSearch","query":{"type":"npm","keywords":["x"]}});
        let state = json!({"results":[{"data":{"artifacts":[{"type":"npm","name":"ajv"}]}}]});
        let items = split(&packages, &state).unwrap();
        assert_eq!(items[0].item.as_deref(), Some("npm:ajv"));
        assert_eq!(
            items[0].read.as_ref().unwrap()["query"]["packageName"],
            "ajv"
        );
        let exact = json!({"tool":"artifactSearch","query":{"type":"npm","packageName":"ajv"}});
        assert!(
            split(&exact, &state).is_none(),
            "an exact lookup is already one item"
        );
    }

    #[test]
    fn empty_lists_and_bare_path_lists_stay_whole() {
        let matches =
            json!({"tool":"astSearch","query":{"operation":"match","pattern":"$X","path":"/repo"}});
        assert!(split(&matches, &wrap(json!({"files":[]}))).is_none());
        let files = json!({"tool":"structureSearch","query":{"operation":"files"}});
        assert!(
            split(
                &files,
                &wrap(json!({"files":[{"dir":"src","files":["a.rs (1)"]}]}))
            )
            .is_none()
        );
        assert!(split(&files, &wrap(json!({"files":[{"path":"a.rs"}]}))).is_none());
        let tree = json!({"tool":"structureSearch","query":{"operation":"tree"}});
        assert!(split(&tree, &wrap(json!({"entries":["a.rs"]}))).is_none());
        let gh_tree = json!({"tool":"ghStructure","query":{"owner":"o","repo":"r"}});
        assert!(split(&gh_tree, &wrap(json!({"entries":[]}))).is_none());
    }
    #[test]
    fn compact_remote_lists_keep_every_candidate_and_its_read() {
        let repos = json!({"tool":"ghSearchRepo","query":{"keywords":["compiler"]}});
        let state = wrap(json!({"repositories":[
            {"owner":"microsoft","repo":"TypeScript","description":"compiler"},
            {"owner":"microsoft","repo":"TypeScript-Compiler-Notes","description":"notes"}
        ]}));
        let items = split(&repos, &state).expect("compact repositories split");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].item.as_deref(), Some("microsoft/TypeScript"));
        assert_eq!(
            items[0].read.as_ref().unwrap()["query"],
            json!({"owner":"microsoft","repo":"TypeScript"})
        );
        let packages = json!({"tool":"artifactSearch","query":{"type":"npm","keywords":["yaml"]}});
        let state =
            wrap(json!({"artifacts":[{"name":"yaml-eslint-parser"},{"name":"yamlparser"}]}));
        let items = split(&packages, &state).expect("compact packages split");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].item.as_deref(), Some("npm:yaml-eslint-parser"));
        assert_eq!(
            items[1].read.as_ref().unwrap()["query"],
            json!({"type":"npm","packageName":"yamlparser"})
        );
    }
}
