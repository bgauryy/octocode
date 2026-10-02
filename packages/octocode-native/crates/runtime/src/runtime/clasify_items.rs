//! Scout over list results. A list page whose items carry content (matches,
//! declarations, references, repositories, pull requests, issues, commits,
//! packages) is judged one item at a time, and every item carries the read
//! that fetches it, so the host reads only the items the judge keeps. Bare
//! path lists (file listings, trees) stay one page: a name alone is judged
//! better comparatively, with a `choice` over the page. Text search keeps its
//! own candidate path (`search_candidate_states`) because it can hydrate file
//! chunks.
use crate::tools::id::ToolId;
use serde_json::{Value, json};

/// One candidate: the narrowed state the provider judges, its identity, and
/// the executable read that fetches it.
pub(super) struct Item {
    pub state: Value,
    pub read: Option<Value>,
    /// Absolute local path or `owner/repo/path` for file candidates.
    pub path: Option<String>,
    /// Identity of a non-file candidate (`owner/repo#12`, `npm:ajv`).
    pub item: Option<String>,
}

/// Split one captured list page into candidates. `None` leaves the page whole:
/// the tool has no candidate list, or the page lists nothing.
pub(super) fn split(source: &Value, state: &Value) -> Option<Vec<Item>> {
    let tool = source
        .get("tool")
        .and_then(Value::as_str)
        .and_then(ToolId::from_name)?;
    let query = source.get("query").unwrap_or(&Value::Null);
    let data = state.pointer("/results/0/data")?;
    let base = state.get("base").and_then(Value::as_str);
    let operation = query.get("operation").and_then(Value::as_str);
    let items = match (tool, operation) {
        (ToolId::AstSearch, Some("match")) => ast_matches(state, data, base)?,
        (ToolId::AstSearch, Some("symbols")) if data.get("files").is_some() => {
            symbol_files(state, data, base)?
        }
        (ToolId::AstSearch, Some("symbols")) => declarations(state, data, base, query)?,
        (ToolId::LspSearch, _) => references(state, data, base, query)?,
        (ToolId::GhSearchRepo, _) => repositories(state, data)?,
        (ToolId::GhSearchHistory, _) => history(state, data, query)?,
        (ToolId::ArtifactSearch, _) if query.get("keywords").is_some() => packages(state, data)?,
        _ => return None,
    };
    (!items.is_empty()).then_some(items)
}

/// List tools whose page size bounds the candidates captured, so clasify can
/// cap fan-out before the read runs.
pub(super) fn is_paged_list(source: &Value) -> bool {
    let query = source.get("query").unwrap_or(&Value::Null);
    let operation = query.get("operation").and_then(Value::as_str);
    match source
        .get("tool")
        .and_then(Value::as_str)
        .and_then(ToolId::from_name)
    {
        // Symbols and references group rows by file; capping rows would cut
        // one file's outline, so those pages are bounded by the cell check.
        Some(ToolId::AstSearch) => operation == Some("match"),
        Some(ToolId::GhSearchRepo | ToolId::GhSearchHistory) => true,
        Some(ToolId::ArtifactSearch) => query.get("keywords").is_some(),
        _ => false,
    }
}

/// A directory symbols outline page: declaration rows grouped per file under
/// `files`. The outline pages by rows, so one file can straddle pages.
pub(super) fn is_symbol_outline(source: &Value, state: &Value) -> bool {
    source.get("tool").and_then(Value::as_str) == Some(ToolId::AstSearch.as_str())
        && source.pointer("/query/operation").and_then(Value::as_str) == Some("symbols")
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

/// Absolute path of an outline file; page `base`s can differ between pages.
fn outline_path(state: &Value, file: &Value) -> Option<String> {
    let base = state.get("base").and_then(Value::as_str);
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
    let rows = first.get("declarations")?.as_array()?.clone();
    state
        .pointer_mut("/results/0/data/files")?
        .as_array_mut()?
        .last_mut()?
        .get_mut("declarations")?
        .as_array_mut()?
        .extend(rows);
    Some(files.len() > 1)
}

fn narrowed(state: &Value, pointer: &str, items: Vec<Value>) -> Value {
    let mut narrowed = state.clone();
    if let Some(slot) = narrowed.pointer_mut(pointer) {
        *slot = Value::Array(items);
    }
    narrowed
}

fn absolute(base: Option<&str>, path: &str) -> String {
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
fn read(tool: ToolId, query: serde_json::Map<String, Value>) -> Value {
    json!({"tool":tool.as_str(),"confidence":"high","query":query})
}

/// A local read around the densest run of anchor lines, or the whole file
/// when the item carries no line.
fn local_read(path: &str, lines: Vec<u64>) -> Value {
    let mut query = serde_json::Map::new();
    query.insert("path".into(), json!(path));
    if let Some(center) = super::densest_match_line(lines) {
        let radius = super::HYDRATED_LINE_RADIUS;
        query.insert(
            "startLine".into(),
            json!(center.saturating_sub(radius).max(1)),
        );
        query.insert("endLine".into(), json!(center.saturating_add(radius)));
    }
    read(ToolId::LocalFetch, query)
}

/// The 1-based line a result row names. Compact string rows (symbols
/// outline `"<line>[-<end>] kind name"`, structural match
/// `"<line>[-<end>]\t<value>"`, reference `"<line>:<col> <text>"`) lead with
/// it; object rows carry it under `key`. Every row reader here goes through
/// this accessor, so either row shape yields the same candidate lines.
fn line(value: &Value, key: &str) -> Option<u64> {
    if let Some(text) = value.as_str() {
        let text = text.trim_start();
        let end = text
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len());
        return text[..end].parse().ok();
    }
    value.get(key).and_then(Value::as_u64)
}

fn ast_matches(state: &Value, data: &Value, base: Option<&str>) -> Option<Vec<Item>> {
    let items = data
        .get("files")?
        .as_array()?
        .iter()
        .filter_map(|file| {
            let path = absolute(base, file.get("path")?.as_str()?);
            let lines = file
                .get("matches")
                .and_then(Value::as_array)
                .map(|matches| matches.iter().filter_map(|m| line(m, "line")).collect())
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

/// A directory outline is already grouped per file: one candidate each.
fn symbol_files(state: &Value, data: &Value, base: Option<&str>) -> Option<Vec<Item>> {
    let items = data
        .get("files")?
        .as_array()?
        .iter()
        .filter_map(|file| {
            let path = absolute(base, file.get("path")?.as_str()?);
            let lines = file
                .get("declarations")
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
fn grouped<'a>(rows: &'a [Value], fallback: Option<&'a str>) -> Vec<(&'a str, Vec<Value>)> {
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

fn declarations(
    state: &Value,
    data: &Value,
    base: Option<&str>,
    query: &Value,
) -> Option<Vec<Item>> {
    let rows = data.get("declarations")?.as_array()?;
    let fallback = data
        .get("path")
        .or_else(|| query.get("path"))
        .and_then(Value::as_str);
    let items = grouped(rows, fallback)
        .into_iter()
        .map(|(path, group)| {
            let path = absolute(base, path);
            let lines = group.iter().filter_map(|row| line(row, "line")).collect();
            Item {
                state: narrowed(state, "/results/0/data/declarations", group),
                read: Some(local_read(&path, lines)),
                path: Some(path),
                item: None,
            }
        })
        .collect();
    Some(items)
}

fn references(state: &Value, data: &Value, base: Option<&str>, query: &Value) -> Option<Vec<Item>> {
    let rows = data.pointer("/payload/locations")?.as_array()?;
    let fallback = query.get("uri").and_then(Value::as_str);
    let items = grouped(rows, fallback)
        .into_iter()
        .map(|(path, group)| {
            let path = absolute(base, path.trim_start_matches("file://"));
            let lines = group
                .iter()
                .filter_map(|row| {
                    row.get("displayRange")
                        .and_then(|range| line(range, "startLine"))
                })
                .collect();
            Item {
                state: narrowed(state, "/results/0/data/payload/locations", group),
                read: Some(local_read(&path, lines)),
                path: Some(path),
                item: None,
            }
        })
        .collect();
    Some(items)
}

fn repositories(state: &Value, data: &Value) -> Option<Vec<Item>> {
    let items = data
        .get("repositories")?
        .as_array()?
        .iter()
        .filter_map(|repository| {
            let owner = repository.get("owner")?.as_str()?;
            let repo = repository.get("repo")?.as_str()?;
            let mut query = serde_json::Map::new();
            query.insert("owner".into(), json!(owner));
            query.insert("repo".into(), json!(repo));
            Some(Item {
                state: narrowed(
                    state,
                    "/results/0/data/repositories",
                    vec![repository.clone()],
                ),
                read: Some(read(ToolId::GhStructure, query)),
                path: None,
                item: Some(format!("{owner}/{repo}")),
            })
        })
        .collect();
    Some(items)
}

/// Owner and repository of a history item: its own fields when a search spans
/// repositories, else the searched repository.
fn item_repository(row: &Value, query: &Value) -> Option<(String, String)> {
    if let Some(full) = row.get("repository").and_then(|repository| {
        repository
            .as_str()
            .or_else(|| repository.get("fullName")?.as_str())
    }) && let Some((owner, repo)) = full.split_once('/')
    {
        return Some((owner.to_owned(), repo.to_owned()));
    }
    let owner = row.get("owner").or_else(|| query.get("owner"))?.as_str()?;
    let repo = row.get("repo").or_else(|| query.get("repo"))?.as_str()?;
    Some((owner.to_owned(), repo.to_owned()))
}

fn history(state: &Value, data: &Value, query: &Value) -> Option<Vec<Item>> {
    let (key, operation) = [
        ("pullRequests", "pullRequest"),
        ("issues", "issue"),
        ("commits", "commit"),
    ]
    .into_iter()
    .find(|(key, _)| data.get(*key).is_some_and(Value::is_array))?;
    let pointer = format!("/results/0/data/{key}");
    let items = data[key]
        .as_array()?
        .iter()
        .map(|row| {
            let located = item_repository(row, query);
            let mut fetch = serde_json::Map::new();
            fetch.insert("operation".into(), json!(operation));
            let identity = match (operation, &located) {
                ("commit", Some((owner, repo))) => {
                    row.get("sha").and_then(Value::as_str).map(|sha| {
                        fetch.insert("ref".into(), json!(sha));
                        format!(
                            "{owner}/{repo}@{}",
                            sha.chars().take(12).collect::<String>()
                        )
                    })
                }
                (_, Some((owner, repo))) => {
                    row.get("number").and_then(Value::as_u64).map(|number| {
                        fetch.insert("number".into(), json!(number));
                        format!("{owner}/{repo}#{number}")
                    })
                }
                _ => None,
            };
            let read = identity.as_ref().and(located).map(|(owner, repo)| {
                fetch.insert("owner".into(), json!(owner));
                fetch.insert("repo".into(), json!(repo));
                read(ToolId::GhGetHistoryItem, fetch)
            });
            Item {
                state: narrowed(state, &pointer, vec![row.clone()]),
                read,
                path: None,
                item: identity,
            }
        })
        .collect();
    Some(items)
}

fn packages(state: &Value, data: &Value) -> Option<Vec<Item>> {
    let items = data
        .get("artifacts")?
        .as_array()?
        .iter()
        .filter_map(|artifact| {
            let kind = artifact.get("type")?.as_str()?;
            let name = artifact.get("name")?.as_str()?;
            let mut query = serde_json::Map::new();
            query.insert("type".into(), json!(kind));
            query.insert("packageName".into(), json!(name));
            Some(Item {
                state: narrowed(state, "/results/0/data/artifacts", vec![artifact.clone()]),
                read: Some(read(ToolId::ArtifactSearch, query)),
                path: None,
                item: Some(format!("{kind}:{name}")),
            })
        })
        .collect();
    Some(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(data: Value) -> Value {
        json!({"base":"/repo","results":[{"data":data}]})
    }

    #[test]
    fn ast_matches_split_per_file_with_a_follow_up_read() {
        let source =
            json!({"tool":"astSearch","query":{"operation":"match","path":"/repo","goal":"g"}});
        let state = wrap(json!({"files":[
            {"path":"a.rs","matches":[{"line":40}]},{"path":"/abs/c.rs","matches":[]}
        ]}));
        let items = split(&source, &state).expect("match split");
        let paths: Vec<_> = items.iter().filter_map(|item| item.path.clone()).collect();
        assert_eq!(paths, ["/repo/a.rs", "/abs/c.rs"]);
        let read = items[0].read.as_ref().unwrap();
        assert_eq!(read["tool"], "localFetch");
        assert_eq!(read["query"]["startLine"], 1);
        assert_eq!(read["query"]["endLine"], 100);
        assert!(
            read["query"].get("goal").is_none(),
            "the matrix brief is added later"
        );
        assert!(
            items[1].read.as_ref().unwrap()["query"]
                .get("startLine")
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
        let page = |files: Value| json!({"base":"/r","results":[{"data":{"files":files}}]});
        let source = json!({"tool":"astSearch","query":{"operation":"symbols","path":"/r"}});
        let mut first = page(json!([
            {"path":"a.rs","declarations":[{"name":"a1","line":1}]},
            {"path":"b.rs","declarations":[{"name":"b1","line":1}]}
        ]));
        assert!(is_symbol_outline(&source, &first));
        let only_b = page(json!([{"path":"b.rs","declarations":[{"name":"b2","line":2}]}]));
        assert_eq!(extend_last_outline_file(&mut first, &only_b), Some(false));
        let b_then_c = json!({"base":"/","results":[{"data":{"files":[
            {"path":"r/b.rs","declarations":[{"name":"b3","line":3}]},
            {"path":"r/c.rs","declarations":[{"name":"c1","line":1}]}
        ]}}]});
        assert_eq!(extend_last_outline_file(&mut first, &b_then_c), Some(true));
        assert_eq!(
            first["results"][0]["data"]["files"][1]["declarations"]
                .as_array()
                .map(Vec::len),
            Some(3)
        );
        let c_only = page(json!([{"path":"c.rs","declarations":[{"name":"c1","line":1}]}]));
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
        let state = wrap(json!({"declarations":[
            {"name":"a","line":10,"path":"x.rs"},{"name":"b","line":300,"path":"y.rs"},{"name":"c","line":20,"path":"x.rs"}
        ]}));
        let items = split(&symbols, &state).expect("symbols split");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].path.as_deref(), Some("/repo/x.rs"));
        assert_eq!(
            items[0].state["results"][0]["data"]["declarations"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(items[1].read.as_ref().unwrap()["query"]["startLine"], 240);

        let grouped = wrap(json!({"files":[
            {"path":"x.rs","declarations":[{"name":"a","line":10},{"name":"c","line":20}]},
            {"path":"y.rs","declarations":[{"name":"b","line":300}]}
        ]}));
        let items = split(&symbols, &grouped).expect("grouped symbols split");
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].path.as_deref(), Some("/repo/y.rs"));
        assert_eq!(items[1].read.as_ref().unwrap()["query"]["startLine"], 240);

        let single =
            json!({"tool":"astSearch","query":{"operation":"symbols","path":"/repo/one.rs"}});
        let state = json!({"results":[{"data":{"path":"/repo/one.rs","declarations":[{"name":"a","line":1}]}}]});
        let items = split(&single, &state).expect("single file");
        assert_eq!(items[0].path.as_deref(), Some("/repo/one.rs"));

        let refs =
            json!({"tool":"lspSearch","query":{"operation":"references","uri":"/repo/a.rs"}});
        let state = wrap(json!({"payload":{"locations":[
            {"displayRange":{"startLine":5},"path":"a.rs"},{"displayRange":{"startLine":9},"path":"b.rs"}
        ]}}));
        let items = split(&refs, &state).expect("refs split");
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].path.as_deref(), Some("/repo/b.rs"));
        let hover = wrap(json!({"payload":{"kind":"hover"}}));
        assert!(split(&refs, &hover).is_none());
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

        let orgwide = json!({"tool":"ghSearchHistory","query":{"operation":"issue"}});
        let state = json!({"results":[{"data":{"issues":[{"number":1}]}}]});
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
        let matches = json!({"tool":"astSearch","query":{"operation":"match"}});
        assert!(split(&matches, &wrap(json!({"files":[]}))).is_none());
        let files = json!({"tool":"structureSearch","query":{"operation":"files"}});
        assert!(split(&files, &wrap(json!({"files":[{"path":"a.rs"}]}))).is_none());
        let tree = json!({"tool":"structureSearch","query":{"operation":"tree"}});
        assert!(split(&tree, &wrap(json!({"entries":["a.rs"]}))).is_none());
        let gh_tree = json!({"tool":"ghStructure","query":{"owner":"o","repo":"r"}});
        assert!(split(&gh_tree, &wrap(json!({"structure":[]}))).is_none());
    }
}
