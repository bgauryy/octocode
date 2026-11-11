//! The fixture worlds: one workspace and runtime per provider double, and
//! the calls each can answer (tool, operation, state).

use crate::support::{self, PROVIDER_LIMITS, Workspace, recorded};
use octocode_native::execution::RuntimeLimits;
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const NEVER: &str = "zzqqxxNeverMatchOctocode";
/// A response page small enough to cut every fixture's answer.
const SMALL_PAGE: u64 = 200;

/// One call and the row state it must produce: `hit`, `empty`, `error`,
/// `outOfRange`, or `paged` (a hit with more to read).
pub struct Case {
    pub tool: String,
    pub operation: String,
    pub state: &'static str,
    pub input: Value,
}

pub struct World {
    pub workspace: Workspace,
    pub runtime: ToolRuntime,
    pub cases: Vec<Case>,
    _servers: Vec<MockServer>,
}

fn case(tool: &str, state: &'static str, row: Value) -> Case {
    Case {
        tool: tool.to_owned(),
        operation: row["operation"].as_str().unwrap_or("-").to_owned(),
        state,
        input: support::envelope(row),
    }
}

/// The same call under a response page that cannot hold its answer.
fn paged(tool: &str, row: Value) -> Case {
    Case {
        input: json!({"queries": [row], "responseChars": SMALL_PAGE}),
        ..case(tool, "paged", row.clone())
    }
}

/// Four modules that import each other, and one file longer than a page.
fn write_sources(workspace: &Workspace) {
    workspace.write(
        "src/a.ts",
        "// first module\nexport function needleA(): number {\n  return 1;\n}\n",
    );
    for (name, from) in [("b", "a"), ("c", "b"), ("d", "c")] {
        let (upper, from_upper) = (name.to_uppercase(), from.to_uppercase());
        workspace.write(
            &format!("src/{name}.ts"),
            format!(
                "import {{ needle{from_upper} }} from \"./{from}\";\n// module {name}\nexport function needle{upper}(): number {{\n  return needle{from_upper}() + 1;\n}}\n"
            ),
        );
    }
    let long: String = (1..=120)
        .map(|line| format!("export const line{line} = {line};\n"))
        .collect();
    workspace.write("src/long.ts", long);
}

/// A language-server double for `.ts`: one definition, four references, in
/// the file asked about. No real server starts.
fn install_language_server(workspace: &Workspace) {
    let script = workspace.write(
        "server.cjs",
        r"
let buffer = Buffer.alloc(0);
function send(id, result) {
  const body = JSON.stringify({jsonrpc:'2.0', id, result});
  process.stdout.write('Content-Length: ' + Buffer.byteLength(body) + '\r\n\r\n' + body);
}
function handle(msg) {
  if (msg.method === 'initialize') return send(msg.id, {capabilities:{definitionProvider:true,referencesProvider:true,textDocumentSync:1}});
  if (msg.method === 'exit') return process.exit(0);
  if (!msg.id) return;
  const uri = msg.params?.textDocument?.uri;
  const location = line => ({uri,range:{start:{line,character:0},end:{line,character:6}}});
  if (msg.method === 'textDocument/definition') return send(msg.id, [location(1)]);
  if (msg.method === 'textDocument/references') return send(msg.id, [0,1,2,3].map(location));
  send(msg.id, null);
}
process.stdin.on('data', chunk => {
  buffer = Buffer.concat([buffer,chunk]);
  for (;;) {
    const split = buffer.indexOf('\r\n\r\n');
    if (split < 0) return;
    const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,split).toString())[1]);
    if (buffer.length < split + 4 + length) return;
    const msg = JSON.parse(buffer.subarray(split+4,split+4+length));
    buffer = buffer.subarray(split+4+length);
    handle(msg);
  }
});
",
    );
    std::fs::write(
        workspace.home.join("lsp-servers.json"),
        json!({"languageServers":{".ts":{
            "command":"node","args":[script],"languageId":"plaintext"
        }}})
        .to_string(),
    )
    .expect("lsp config");
}

fn workspace() -> Workspace {
    let workspace = Workspace::new();
    write_sources(&workspace);
    install_language_server(&workspace);
    workspace
}

fn search_cases() -> Vec<Case> {
    let search = json!({"path":"src","matchString":["needle"]});
    let with = |extra: Value| merged(&search, &extra);
    vec![
        case("localSearch", "hit", search.clone()),
        case("localSearch", "hit", with(json!({"view":"files"}))),
        case("localSearch", "hit", with(json!({"view":"countMatches"}))),
        case("localSearch", "paged", with(json!({"limit":1}))),
        case(
            "localSearch",
            "empty",
            json!({"path":"src","matchString":[NEVER]}),
        ),
        case(
            "localSearch",
            "error",
            json!({"path":"missing","matchString":["x"]}),
        ),
        case("localFetch", "hit", json!({"path":"src/a.ts"})),
        paged("localFetch", json!({"path":"src/long.ts"})),
        case("localFetch", "error", json!({"path":"missing.ts"})),
        case(
            "localFetch",
            "outOfRange",
            json!({"path":"src/a.ts","ranges":[{"startLine":900,"endLine":901}]}),
        ),
        case(
            "structureSearch",
            "hit",
            json!({"operation":"tree","path":"src"}),
        ),
        case(
            "structureSearch",
            "hit",
            json!({"operation":"files","path":"src"}),
        ),
        case(
            "structureSearch",
            "paged",
            json!({"operation":"files","path":"src","limit":2}),
        ),
        case(
            "structureSearch",
            "empty",
            json!({"operation":"files","path":"src","include":[NEVER]}),
        ),
        case(
            "structureSearch",
            "error",
            json!({"operation":"tree","path":"missing"}),
        ),
    ]
}

fn code_cases() -> Vec<Case> {
    let anchor = json!({"path":"src/a.ts","symbolName":"needleA","line":2});
    let lsp = |extra: Value| merged(&anchor, &extra);
    let never = json!({"terms":[NEVER],"mode":"exact"});
    vec![
        case(
            "astSearch",
            "hit",
            json!({"operation":"symbols","path":"src"}),
        ),
        case(
            "astSearch",
            "hit",
            json!({"operation":"match","path":"src","rule":{"kind":"return_statement"}}),
        ),
        case(
            "astSearch",
            "paged",
            json!({"operation":"symbols","path":"src","limit":2}),
        ),
        case(
            "astSearch",
            "empty",
            json!({"operation":"symbols","path":"src","symbolFilter":never}),
        ),
        case(
            "astSearch",
            "error",
            json!({"operation":"symbols","path":"missing"}),
        ),
        case("lspSearch", "hit", lsp(json!({"operation":"definition"}))),
        case("lspSearch", "hit", lsp(json!({"operation":"references"}))),
        case(
            "lspSearch",
            "hit",
            lsp(json!({"operation":"references","view":"files"})),
        ),
        case(
            "lspSearch",
            "paged",
            lsp(json!({"operation":"references","limit":2})),
        ),
        case(
            "lspSearch",
            "error",
            json!({"path":"missing.ts","symbolName":"x","line":1,"operation":"definition"}),
        ),
    ]
}

/// graphQuery reads the snapshot graphBuild writes, so the build runs first.
fn graph_cases() -> Vec<Case> {
    let query = |extra: Value| merged(&json!({"path":"src"}), &extra);
    vec![
        case("graphBuild", "hit", json!({"path":"src"})),
        case("graphQuery", "hit", query(json!({"operation":"stats"}))),
        case(
            "graphQuery",
            "hit",
            query(json!({"operation":"stats","view":"details"})),
        ),
        case(
            "graphQuery",
            "hit",
            query(json!({"operation":"find","target":"needle"})),
        ),
        case(
            "graphQuery",
            "hit",
            query(json!({"operation":"symbols","target":"a.ts"})),
        ),
        case(
            "graphQuery",
            "hit",
            query(json!({"operation":"dependents","target":"a.ts"})),
        ),
        case(
            "graphQuery",
            "hit",
            query(json!({"operation":"hubs","kind":"file"})),
        ),
        case(
            "graphQuery",
            "paged",
            query(json!({"operation":"find","target":"needle","limit":1})),
        ),
        // Functions and one-line constants in one list: rows of two builds.
        case(
            "graphQuery",
            "paged",
            query(json!({"operation":"find","kind":"symbol"})),
        ),
        case(
            "graphQuery",
            "empty",
            query(json!({"operation":"find","target":NEVER})),
        ),
        case(
            "graphQuery",
            "error",
            json!({"operation":"stats","path":"missing"}),
        ),
    ]
}

fn merged(base: &Value, extra: &Value) -> Value {
    let mut row = base.clone();
    for (key, value) in extra.as_object().into_iter().flatten() {
        row[key] = value.clone();
    }
    row
}

/// The local tools over the fixture workspace: no provider, no network.
pub fn local() -> World {
    let workspace = workspace();
    let runtime = workspace.runtime(&[("OCTOCODE_BETA", "true".into())]);
    let mut cases = search_cases();
    cases.extend(code_cases());
    cases.extend(graph_cases());
    World {
        workspace,
        runtime,
        cases,
        _servers: Vec::new(),
    }
}

/// A recorded provider answer (`test-utils/provider-answers`) on one double
/// that serves GitHub, OSV and an npm registry by path, in the `state` the
/// recorded answer produces. `with_paged` adds the same call under a small
/// response page.
pub async fn recorded(name: &str, state: &'static str, with_paged: bool) -> World {
    let server = MockServer::builder().start().await;
    let recording = recorded::mount(&server, name, |url| match url.host_str() {
        Some("api.github.com") => recorded::github_path(url),
        _ => url.path().to_owned(),
    })
    .await;
    let workspace = workspace();
    let osv_api: &'static str = Box::leak(format!("{}/v1", server.uri()).into_boxed_str());
    let mut env = support::github_api(&server).to_vec();
    env.extend([
        ("OCTOCODE_SECURITY_CHECKS", "true".to_owned()),
        ("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".to_owned()),
    ]);
    let runtime = workspace.runtime_with(
        &env,
        RuntimeLimits {
            osv_api,
            ..PROVIDER_LIMITS
        },
    );
    let tool = recording["tool"].as_str().expect("recorded tool");
    let mut row = recording["query"].clone();
    // Only artifactSearch `resolve` takes the registry a caller names.
    if row["operation"] == "resolve" {
        row["registryUrl"] = json!(server.uri());
    }
    let mut cases = vec![case(tool, state, row.clone())];
    // A recording made with `view:"details"` (every field the provider
    // sends) is also asked in the default view, the summary.
    if let Some(fields) = row
        .as_object()
        .filter(|fields| fields.get("view") == Some(&json!("details")))
    {
        let mut summary = fields.clone();
        summary.remove("view");
        cases.push(case(tool, state, Value::Object(summary)));
    }
    if with_paged {
        cases.push(paged(tool, row));
    }
    World {
        workspace,
        runtime,
        cases,
        _servers: vec![server],
    }
}

/// GitHub and a registry that find nothing: the empty and failed rows of
/// every remote tool, and a clone the provider refuses.
pub async fn nothing_found() -> World {
    let server = crate::runtime_empty_error_rows::github_and_registry().await;
    let workspace = workspace();
    let mut env = support::github_api(&server).to_vec();
    env.extend([
        ("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".to_owned()),
        ("OCTOCODE_ENABLE_CLONE", "true".to_owned()),
    ]);
    let runtime = workspace.runtime_with(&env, PROVIDER_LIMITS);
    let found = json!({"owner":"a","repo":"b"});
    let missing = json!({"owner":"a","repo":"missing"});
    let package = NEVER.to_lowercase();
    let cases = vec![
        case(
            "ghSearchRepo",
            "empty",
            json!({"operation":"search","keywords":[NEVER]}),
        ),
        case(
            "ghSearchCode",
            "empty",
            merged(&found, &json!({"keywords":[NEVER]})),
        ),
        case(
            "ghSearchHistory",
            "empty",
            merged(
                &found,
                &json!({"operation":"pullRequest","keywords":[NEVER]}),
            ),
        ),
        case(
            "ghStructure",
            "error",
            merged(&missing, &json!({"operation":"tree"})),
        ),
        case(
            "ghGetFileContent",
            "error",
            merged(&missing, &json!({"path":"missing.md"})),
        ),
        case(
            "ghGetHistoryItem",
            "error",
            merged(
                &missing,
                &json!({"operation":"pullRequest","number":999_999}),
            ),
        ),
        case("ghCloneRepo", "error", missing),
        case(
            "artifactSearch",
            "error",
            json!({"operation":"resolve","ecosystem":"npm","packageName":package,"registryUrl":server.uri()}),
        ),
    ];
    World {
        workspace,
        runtime,
        cases,
        _servers: vec![server],
    }
}

/// Answers every noul question 0.9, as the classification provider does.
struct Judged;

impl Respond for Judged {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).expect("provider body");
        let answers: serde_json::Map<_, _> = body["questions"]
            .as_object()
            .expect("questions")
            .keys()
            .map(|id| (id.clone(), json!({"type":"noul","noul":0.9})))
            .collect();
        ResponseTemplate::new(200).set_body_json(json!({
            "model":"jev-1.13.0","answers":answers,"usage":{"input_tokens":40,"output_tokens":7}
        }))
    }
}

/// clasify over a classification-provider double.
pub async fn judged() -> World {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(Judged)
        .mount(&server)
        .await;
    let workspace = workspace();
    let runtime = support::clasify_runtime(&workspace, &server, &[]);
    let questions =
        json!({"exports":{"type":"noul","instructions":"Does the file export a function?"}});
    let evaluate = |source: &str| {
        json!({
            "operation":"evaluate",
            "source":{"tool":"localFetch","query":{"path":source}},
            "questions":questions.clone()
        })
    };
    let cases = vec![
        case("clasify", "hit", evaluate("src/a.ts")),
        case(
            "clasify",
            "hit",
            json!({
                "operation":"sections",
                "resources":{"long":{"tool":"localFetch","query":{"path":"src/long.ts"}}},
                "questions":questions.clone()
            }),
        ),
        case("clasify", "error", evaluate("missing.ts")),
    ];
    World {
        workspace,
        runtime,
        cases,
        _servers: vec![server],
    }
}

/// Every world, local first. Each runs without retries, so a call a double
/// does not answer fails fast.
pub async fn worlds() -> Vec<World> {
    let mut worlds = vec![local(), nothing_found().await, judged().await];
    for (name, state, with_paged) in [
        ("github/repo-get", "hit", false),
        ("github/code-search", "hit", false),
        ("github/structure-tree", "hit", false),
        ("github/file", "hit", false),
        ("github/history-commits", "hit", true),
        ("github/history-pulls", "paged", false),
        ("github/item-commit", "hit", true),
        ("github/item-pull", "hit", false),
        ("npm/exact", "hit", false),
        ("osv/advisory-ghsa", "hit", true),
        ("osv/advisory-unknown", "error", false),
        ("osv/commit", "hit", false),
    ] {
        worlds.push(recorded(name, state, with_paged).await);
    }
    worlds
}
