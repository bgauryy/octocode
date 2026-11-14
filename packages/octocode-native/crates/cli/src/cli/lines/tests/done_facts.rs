//! Rules every tool's text obeys, checked over one set of results shaped as
//! the tools return them: stdout alone says whether the answer is whole,
//! both streams together carry every JSON leaf, text is no larger than its
//! JSON, and a row prints alike alone and in a batch.
use super::*;

/// A tool, the query it ran and its result.
type Case = (ToolId, Value, Value);

fn next(tool: &str, cursor: &str) -> Value {
    json!({"nextPage":{"kind":"continue","tool":tool,"input":{"cursor":cursor}}})
}

fn related(tool: &str, path: &str) -> Value {
    json!([{"kind":"related","tool":tool,"input":{"queries":[{"path":path}]}}])
}

fn one(data: Value) -> Value {
    json!({"results":[{"index":0,"data":data}]})
}

fn with_status(mut result: Value, status: &str) -> Value {
    result["results"][0]["status"] = status.into();
    result
}

/// Single-row results of several tools: pages, empties, failures, listings,
/// sites, outlines and plain `key: value` rows.
fn cases() -> Vec<Case> {
    let dir = "packages/native/crates/cli/src/cli/lines";
    let search = json!({"queries":[{"path":"src","matchString":["fn "]}]});
    let mut cases = vec![
        (
            ToolId::LocalSearch,
            search.clone(),
            one(json!({"matchCount":58,"filesScanned":9,
                "files":[{"path":format!("{dir}/outline/tests.rs"),"matches":[
                    {"line":6,"value":"fn leaves(value: &Value) {"},
                    {"line":17,"value":"fn count_lines() {"}],
                    "pagination":{"unit":"matches","offset":0,"length":2,"total":9,"hasMore":true}}],
                "pagination":{"unit":"lines","offset":0,"length":3,"total":58,"hasMore":true},
                "next":next("localSearch","cursor2.search-page"),
                "related":related("localFetch","src/a.rs")})),
        ),
        (
            ToolId::LocalSearch,
            search.clone(),
            with_status(
                one(json!({"matchCount":0,"filesScanned":9,
                    "related":related("localSearch",".")})),
                "empty",
            ),
        ),
        (
            ToolId::LocalSearch,
            search.clone(),
            with_status(
                one(
                    json!({"errorCode":"notFound","error":"Path does not exist: src/nope",
                    "related":related("structureSearch","src")}),
                ),
                "error",
            ),
        ),
        (
            ToolId::LocalSearch,
            json!({"queries":[{"path":"src","matchString":["fn "],"view":"countMatches"}]}),
            one(json!({"matchCount":17,"files":[
                {"path":format!("{dir}/outline/tests.rs"),"matchCount":9},
                {"path":format!("{dir}/listing.rs"),"matchCount":8}]})),
        ),
        (
            ToolId::LocalSearch,
            json!({"queries":[{"path":"src","matchString":["fn "],"view":"files"}]}),
            one(json!({"matchedFiles":2,"files":[
                {"path":format!("{dir}/outline/tests.rs")},{"path":format!("{dir}/listing.rs")}]})),
        ),
        (
            ToolId::StructureSearch,
            json!({"queries":[{"path":".","operation":"files"}]}),
            one(json!({"files":[
                {"dir":dir,"files":["follow.rs (4680, totalLines=132)","hits.rs (5453, totalLines=140)","outline/"]},
                {"dir":format!("{dir}/outline"),"files":["tests.rs (7354, totalLines=184)"]}],
                "pagination":{"unit":"items","offset":0,"length":4,"total":5015,"hasMore":true},
                "summary":{"fileCount":4975,"dirCount":922},
                "next":next("structureSearch","cursor2.listing-page"),
                "related":related("localFetch","AGENTS.md")})),
        ),
        (
            ToolId::StructureSearch,
            json!({"queries":[{"path":".","operation":"tree"}]}),
            one(
                json!({"files":[{"dir":dir,"files":["follow.rs","hits.rs","listing.rs","outline/","outline.rs"]}],
                "summary":{"fileCount":4,"dirCount":1,"bytes":29702}}),
            ),
        ),
        (
            ToolId::StructureSearch,
            json!({"queries":[{"path":".","operation":"files","nameRegex":"zzz"}]}),
            with_status(
                one(json!({"files":[],"hints":["No entry matches nameRegex."]})),
                "empty",
            ),
        ),
    ];
    cases.extend(more_cases(dir));
    cases
}

/// The rest of [`cases`]: listings of another tool, outlines, sites, reads
/// and plain `key: value` rows.
fn more_cases(dir: &str) -> Vec<Case> {
    vec![
        (
            ToolId::GhStructure,
            json!({"queries":[{"operation":"tree","owner":"facebook","repo":"react","path":"packages"}]}),
            one(
                json!({"owner":"react","files":[{"dir":"packages","files":["jest-react/","react/","react-art/","react-dom/"]}],
                "summary":{"fileCount":0,"dirCount":40,"bytes":0},"ref":"main",
                "commitSha":"b05c0b75fd9778536152eead30814c9ca90c810a",
                "pagination":{"unit":"items","offset":0,"length":20,"total":40,"hasMore":true},
                "next":next("ghStructure","cursor2.tree-page")}),
            ),
        ),
        (
            ToolId::AstSearch,
            json!({"queries":[{"path":format!("{dir}/follow.rs"),"operation":"symbols"}]}),
            one(json!({"filesScanned":1,"symbols":[
                {"symbolName":"field_notes","kind":"function","line":5,"endLine":26,"docStartLine":4},
                {"symbolName":"separate_calls","kind":"function","line":29,"endLine":79},
                {"symbolName":"Call","kind":"impl","line":81,"endLine":107,
                 "members":[{"symbolName":"note","kind":"method","line":83,"endLine":85}]}]})),
        ),
        (
            ToolId::AstSearch,
            json!({"queries":[{"path":format!("{dir}/follow.rs"),"operation":"syntaxTree"}]}),
            one(
                json!({"nodes":[{"node":"0 source_file 1:1-133:1"},{"node":"1 use_declaration 1:1-1:18 ^0"},
                {"node":"2 argument:scoped_identifier 1:5-1:17 ^1"}],
                "next":next("astSearch","cursor2.tree-nodes")}),
            ),
        ),
        (
            ToolId::LspSearch,
            json!({"queries":[{"path":"src/budget.rs","operation":"references","symbolName":"deadline","line":29,"view":"files"}]}),
            one(json!({"matchCount":6,"files":[
                {"path":"packages/native/crates/github/src/budget/tests/gates.rs","matchCount":1,"matchedLines":[69]},
                {"path":"packages/native/crates/github/src/commit_dates/tests.rs","matchCount":2,"matchedLines":[5,32]},
                {"path":"packages/native/crates/github/src/history_item.rs","matchCount":3,"matchedLines":[79,122,173]}],
                "pagination":{"unit":"items","offset":0,"length":5,"total":14,"hasMore":true},
                "next":next("lspSearch","cursor2.sites-page")})),
        ),
        (
            ToolId::LspSearch,
            json!({"queries":[{"path":"src/budget.rs","operation":"references","symbolName":"deadline","line":29}]}),
            one(
                json!({"operation":"references","files":[{"path":"a.rs","matches":[
                {"line":2,"content":"2\tneedle();\n","role":"call"},
                {"line":6,"content":"6\tneedle();\n","role":"call"}]}]}),
            ),
        ),
        (
            ToolId::LocalFetch,
            json!({"queries":[{"path":"src/security_check.rs","view":"compact"}]}),
            one(
                json!({"content":"4\tmod advisories;\n5\tmod answer;\n6\tmod commit;","totalLines":411,
                "pagination":{"unit":"lines","offset":0,"length":3,"hasMore":true},
                "next":next("localFetch","cursor2.fetch-page")}),
            ),
        ),
        (
            ToolId::LocalFetch,
            json!({"queries":[{"path":"src/tests.rs","ranges":[{"startLine":100,"endLine":105}]}]}),
            with_status(one(json!({"content":"","totalLines":23})), "outOfRange"),
        ),
        (
            ToolId::GhSearchHistory,
            json!({"queries":[{"operation":"pullRequest","owner":"facebook","repo":"react","keywords":["useEffectEvent"]}]}),
            one(json!({"pullRequests":[
                {"number":37618,"title":"[compiler] Add regression test","mergedAt":"2026-09-16T08:53:44Z"},
                {"number":34831,"title":"[Fiber] Ensure latest values","mergedAt":"2025-10-13T15:58:43Z"}],
                "effectiveQuery":"useEffectEvent is:pr repo:react/react is:merged",
                "pagination":{"unit":"items","offset":0,"length":3,"total":27,"hasMore":true},
                "next":next("ghSearchHistory","cursor2.history-page"),
                "warnings":["Repository facebook/react was renamed to react/react."]})),
        ),
        (
            ToolId::GraphQuery,
            json!({"queries":[{"operation":"find","target":"lease"}]}),
            one(
                json!({"graph":{"id":"20261110T181651Z-src-d9b10b","root":"src","isPartial":true,"partialReasons":["filesSkipped"]},
                "rows":[{"id":"runtime/rows.ts#leaseKind","kind":"symbol","file":"runtime/rows.ts","line":29,"name":"leaseKind"}],
                "pagination":{"unit":"items","offset":0,"length":10,"total":155,"hasMore":true},
                "next":next("graphQuery","cursor2.graph-page")}),
            ),
        ),
    ]
}

/// The envelope's own page over a paged row: the response page runs first.
fn response_paged() -> Case {
    let (tool, query, mut result) = cases().swap_remove(0);
    result["results"][0]["rowPart"] = json!({"part":1,"of":3});
    result["responsePagination"] = json!({"hasMore":true,
        "next":{"kind":"continue","tool":"localSearch","input":{"cursor":"cursor2.response-page"}}});
    (tool, query, result)
}

fn text(case: &Case) -> Lines {
    let (tool, query, result) = case;
    render(*tool, result, query, None).unwrap_or_else(|| panic!("{tool:?} prints text: {result}"))
}

/// Every call of `value` as its command, with whether it is a `related` lead.
pub(super) fn commands(value: &Value, out: &mut Vec<(String, bool)>) {
    match value {
        Value::Object(fields) => {
            if let (Some(tool), Some(input)) = (
                fields.get("tool").and_then(Value::as_str),
                fields.get("input"),
            ) {
                out.push((
                    follow::command(tool, &input.to_string()),
                    fields.get("kind").and_then(Value::as_str) == Some("related"),
                ));
                return;
            }
            fields.values().for_each(|value| commands(value, out));
        }
        Value::Array(items) => items.iter().for_each(|item| commands(item, out)),
        _ => {}
    }
}

/// How many objects of `value` state `hasMore`.
pub(super) fn page_states(value: &Value) -> usize {
    match value {
        Value::Object(fields) => {
            usize::from(fields.contains_key("hasMore"))
                + fields.values().map(page_states).sum::<usize>()
        }
        Value::Array(items) => items.iter().map(page_states).sum(),
        _ => 0,
    }
}

/// Every string line and number of `value` outside its calls; a page's
/// offset 0 is stated by the page's count reading without `after`.
fn evidence(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(fields) if fields.contains_key("tool") && fields.contains_key("input") => {}
        Value::Object(fields) => fields
            .iter()
            .filter(|(key, value)| {
                !(*key == "offset" && **value == 0 && fields.contains_key("hasMore"))
            })
            .for_each(|(_, value)| evidence(value, out)),
        Value::Array(items) => items.iter().for_each(|item| evidence(item, out)),
        Value::String(text) => out.extend(
            text.split('\n')
                .filter(|line| !line.is_empty())
                .map(str::to_owned),
        ),
        Value::Number(number) => out.push(number.to_string()),
        _ => {}
    }
}

/// The directories `text` states its rows' paths relative to (`# root:`,
/// alone or after a batch row's name).
pub(super) fn roots(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| line.strip_prefix("# "))
        .filter_map(|line| match line.split_once("root: ") {
            Some((label, root)) if label.is_empty() || label.starts_with("results[") => Some(root),
            _ => None,
        })
        .collect()
}

/// Whether `leaf` is printed: as it is, as a path relative to a stated root,
/// or a numbered source line (`N<TAB>text`) as the hit `N:text` or the
/// context `N-text`.
fn printed(leaf: &str, text: &str) -> bool {
    text.contains(leaf)
        || roots(text).iter().any(|root| {
            leaf.strip_prefix(&format!("{root}/"))
                .is_some_and(|relative| text.contains(relative))
        })
        || leaf.split_once('\t').is_some_and(|(n, source)| {
            n.bytes().all(|byte| byte.is_ascii_digit())
                && [':', '-']
                    .iter()
                    .any(|sep| text.contains(&format!("{n}{sep}{source}")))
        })
}

/// (a) With stderr dropped, stdout states each row's status, every page
/// state and every command that continues the answer, after the content.
#[test]
fn stdout_alone_says_whether_the_answer_is_whole() {
    for case in cases().into_iter().chain([response_paged()]) {
        let (tool, _, result) = &case;
        let out = text(&case).out;
        let trailer: Vec<&str> = out
            .lines()
            .rev()
            .take_while(|line| line.starts_with("# "))
            .collect();
        let row = &result["results"][0];
        if let Some(status) = row["status"].as_str() {
            assert!(
                trailer.contains(&format!("# status: {status}").as_str()),
                "{tool:?} status: {out}"
            );
        }
        for key in ["errorCode", "error"] {
            if let Some(stated) = row["data"][key].as_str() {
                assert!(out.contains(stated), "{tool:?} {key}: {out}");
            }
        }
        assert!(
            out.matches("hasMore").count() >= page_states(result),
            "{tool:?} page states: {out}"
        );
        if let Some(part) = row.get("rowPart") {
            let note = format!("# rowPart: part {} of {}", part["part"], part["of"]);
            assert!(trailer.contains(&note.as_str()), "{tool:?} rowPart: {out}");
        }
        let mut calls = Vec::new();
        commands(result, &mut calls);
        for (command, _) in calls.iter().filter(|(_, lead)| !lead) {
            assert!(
                trailer.iter().any(|line| line.ends_with(command.as_str())),
                "{tool:?} continuation {command}: {out}"
            );
        }
    }
}

/// The three answers a caller must tell apart read differently on stdout.
#[test]
fn no_hits_a_failure_and_a_first_page_differ_on_stdout() {
    let cases = cases();
    let outs: Vec<String> = cases.iter().take(3).map(|case| text(case).out).collect();
    assert!(outs[0].ends_with(
        "# pagination: 3 of 58 lines hasMore\n# next.nextPage: octocode localSearch '{\"cursor\":\"cursor2.search-page\"}'"
    ), "{}", outs[0]);
    assert_eq!(outs[1], "# status: empty");
    assert_eq!(
        outs[2],
        "errorCode: notFound\nerror: Path does not exist: src/nope\n# status: error"
    );
}

/// The response page runs before the row's own: its command comes first.
#[test]
fn the_response_page_comes_before_the_rows_next_page() {
    let out = text(&response_paged()).out;
    let at = |cursor: &str| {
        out.find(cursor)
            .unwrap_or_else(|| panic!("{cursor}: {out}"))
    };
    assert!(
        at("cursor2.response-page") < at("cursor2.search-page"),
        "{out}"
    );
    assert!(
        out.contains("\n# responsePagination: hasMore\n# responsePagination.next: octocode "),
        "{out}"
    );
}

/// (b) Stdout and stderr together carry every string and number of the JSON
/// and every call as its command; every stderr line is a `# ` note.
#[test]
fn both_streams_carry_every_json_leaf() {
    for case in cases().into_iter().chain([response_paged()]) {
        let (tool, _, result) = &case;
        let lines = text(&case);
        let both = format!("{}\n{}", lines.out, lines.notes);
        // A lone row's index is its place, which a batch's heading states.
        let mut stated = result.clone();
        stated["results"][0]
            .as_object_mut()
            .expect("a row")
            .shift_remove("index");
        let mut leaves = Vec::new();
        evidence(&stated, &mut leaves);
        for leaf in &leaves {
            assert!(printed(leaf, &both), "{tool:?} lost {leaf:?}:\n{both}");
        }
        let mut calls = Vec::new();
        commands(result, &mut calls);
        for (command, lead) in &calls {
            let stream = if *lead { &lines.notes } else { &lines.out };
            assert_eq!(
                stream.matches(command.as_str()).count(),
                1,
                "{tool:?} {command}:\n{both}"
            );
        }
        for note in lines.notes.lines() {
            assert!(note.starts_with("# "), "{tool:?} unprefixed note {note:?}");
        }
    }
}

/// (c) Text is no larger than the JSON of the same result: a path or a
/// directory is stated once, as the JSON groups it.
#[test]
fn text_is_no_larger_than_its_json() {
    for case in cases().into_iter().chain([response_paged()]) {
        let (tool, query, result) = &case;
        let out = text(&case).out;
        let json = result.to_string();
        assert!(
            out.len() <= json.len(),
            "{tool:?} {query}: text {} > JSON {}:\n{out}",
            out.len(),
            json.len()
        );
    }
}

/// A file of many sites or entries states its path once.
#[test]
fn a_path_is_stated_once_per_file() {
    for case in cases() {
        let (tool, _, result) = &case;
        let out = text(&case).out;
        let groups = result["results"][0]["data"]["files"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        let roots = roots(&out);
        for group in groups {
            let Some(path) = group["path"].as_str().or_else(|| group["dir"].as_str()) else {
                continue;
            };
            let path = roots
                .iter()
                .find_map(|root| path.strip_prefix(&format!("{root}/")))
                .unwrap_or(path);
            // A count table's row leads with the count.
            let shown = |line: &str| {
                let line = line.trim_start();
                line.split_once(' ')
                    .filter(|(count, _)| count.bytes().all(|byte| byte.is_ascii_digit()))
                    .map_or(line, |(_, rest)| rest)
                    .to_owned()
            };
            let lines = out
                .lines()
                .map(shown)
                .filter(|line| {
                    line.starts_with(path) && !line.starts_with(&format!("{path}/outline"))
                })
                .count();
            assert_eq!(lines, 1, "{tool:?} {path}: {out}");
        }
    }
}

/// `path:N` names a line in every tool; a count is a named field.
#[test]
fn a_number_after_a_path_is_a_line() {
    let cases = cases();
    let counts = text(&cases[3]).out;
    assert!(!counts.contains(".rs:"), "{counts}");
    assert!(
        counts.lines().any(|line| line.trim() == "8 listing.rs"),
        "{counts}"
    );
    let sites = text(&cases[11]).out;
    assert!(sites.contains("commit_dates/tests.rs:5,32\n"), "{sites}");
    assert!(!sites.contains("matchCount"), "{sites}");
}

/// A list of single-field items names the field once.
#[test]
fn a_repeated_single_field_is_named_once() {
    let out = text(&cases()[10]).out;
    assert!(
        out.starts_with(
            "nodes[].node:\n  - 0 source_file 1:1-133:1\n  - 1 use_declaration 1:1-1:18 ^0\n"
        ),
        "{out}"
    );
    assert_eq!(out.matches("node:").count(), 1, "{out}");
}

/// Whether `row` prints alike alone and in a batch of itself twice, apart
/// from its `## results[N]` heading, its `results[N]` name on each of its
/// `# ` lines, and the fields both rows share, which print once before the
/// first row exactly as a row of only those fields prints them.
pub(super) fn prints_alike_in_a_batch(tool: ToolId, row: &Value, asked: &Value) -> bool {
    let mut second = row.clone();
    second["index"] = 1.into();
    let batch = render(
        tool,
        &json!({"results": [row, second]}),
        &json!({"queries": [asked, asked]}),
        None,
    );
    let Some(batch) = batch else {
        return false;
    };
    let (block, _) = batch.out.split_once("## results[0]").unwrap_or_default();
    let block = block.trim_end_matches('\n');
    // The fields the block states: its unindented `key:` heads.
    let shared: Vec<&str> = block
        .lines()
        .filter(|line| !line.starts_with(' '))
        .filter_map(|line| line.split_once(':'))
        // `rows[]: …` (a table) and `nodes[].node:` name the field `rows`, `nodes`.
        .filter_map(|(key, _)| key.split('[').next())
        .collect();
    let mut own = row.clone();
    let mut fields = Map::new();
    if let Some(data) = own["data"].as_object_mut() {
        for key in &shared {
            fields.extend(
                data.shift_remove(*key)
                    .map(|value| ((*key).to_owned(), value)),
            );
        }
    }
    if !block.is_empty() {
        let alone = render(
            tool,
            &json!({"results":[{"data": fields}]}),
            &json!({}),
            None,
        );
        if alone.is_none_or(|alone| alone.out != block) {
            return false;
        }
    }
    let Some(alone) = render(
        tool,
        &json!({"results": [own]}),
        &json!({"queries": [asked]}),
        None,
    ) else {
        return false;
    };
    let named = |text: &str, label: &str| -> Vec<String> {
        text.lines()
            .map(|line| match line.strip_prefix("# ") {
                Some(rest) => format!("# {label} {rest}"),
                None => line.to_owned(),
            })
            .collect()
    };
    let mut out: Vec<String> = block.lines().map(str::to_owned).collect();
    let mut notes = Vec::new();
    for label in ["results[0]", "results[1]"] {
        out.push(format!("## {label}"));
        out.extend(named(&alone.out, label));
        notes.extend(named(&alone.notes, label));
    }
    batch.out == out.join("\n") && batch.notes == notes.join("\n")
}

/// (d) A row prints alike alone and in a batch, apart from its heading, its
/// name on its `# ` lines and the fields every row shares.
#[test]
fn a_row_prints_alike_alone_and_in_a_batch() {
    for (tool, query, result) in cases() {
        assert!(
            prints_alike_in_a_batch(tool, &result["results"][0], &query["queries"][0]),
            "{tool:?} {result}"
        );
    }
}

/// An empty or failed batch row states its status under its heading.
#[test]
fn a_batch_row_without_content_states_its_status() {
    let cases = cases();
    let mut failed = cases[2].2["results"][0].clone();
    failed["index"] = 1.into();
    let batch = json!({"results": [cases[1].2["results"][0], failed]});
    let lines = render(ToolId::LocalSearch, &batch, &json!({}), None).expect("text");
    assert_eq!(
        lines.out,
        "## results[0]\n# results[0] status: empty\n## results[1]\nerrorCode: notFound\nerror: Path does not exist: src/nope\n# results[1] status: error"
    );
    // One grammar for every note: the row, then `key: value`.
    for note in lines.notes.lines() {
        let rest = note
            .strip_prefix("# results[0] ")
            .or_else(|| note.strip_prefix("# results[1] "))
            .unwrap_or_else(|| panic!("{note}"));
        let key = rest.split(": ").next().unwrap_or_default();
        assert!(!key.contains("results") && !key.contains("data."), "{note}");
    }
    assert!(
        lines
            .notes
            .contains("# results[0] related[0]: octocode localSearch "),
        "{}",
        lines.notes
    );
    assert!(
        lines
            .notes
            .contains("# results[1] related[0]: octocode structureSearch "),
        "{}",
        lines.notes
    );
}
