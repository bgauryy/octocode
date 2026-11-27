//! A commit's files read many to a GraphQL request: each file the answer
//! vouches for is then read with nothing sent, and every file it cannot vouch
//! for (a link, a binary, a cut or altered text, an alias error) is read by
//! REST as before, from the same bytes GitHub serves there.
use super::links::keeping_provider;
use super::*;
use octocode_test_support::mock_http::GitRepository;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use wiremock::{Request, Respond};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// The git object id of a blob holding `text`.
fn oid(text: &str) -> String {
    use sha1::Digest as _;
    let mut digest = sha1::Sha1::new();
    digest.update(format!("blob {}\0", text.len()).as_bytes());
    digest.update(text.as_bytes());
    hex::encode(digest.finalize())
}

/// GraphQL's view of one repository at [`COMMIT`]: each file's blob as
/// GitHub answers it (`blobs`, which a test may alter) and each folder's tree.
#[derive(Clone, Default)]
struct Answers {
    /// Path → (git mode, blob answer).
    files: BTreeMap<String, (u32, Value)>,
    /// Aliases answered with an error naming them.
    failing: Vec<String>,
    /// Folders whose object is no tree (answered `{}`).
    not_trees: Vec<String>,
}

impl Answers {
    fn file(mut self, path: &str, text: &str) -> Self {
        self.files.insert(
            path.to_owned(),
            (
                0o100_644,
                json!({"oid": oid(text), "byteSize": text.len(), "isBinary": false, "isTruncated": false, "text": text}),
            ),
        );
        self
    }

    fn altered(mut self, path: &str, change: impl FnOnce(&mut Value)) -> Self {
        change(&mut self.files.get_mut(path).expect("file").1);
        self
    }

    fn mode(mut self, path: &str, mode: u32) -> Self {
        self.files.get_mut(path).expect("file").0 = mode;
        self
    }

    fn entries(&self, dir: &str) -> Vec<Value> {
        let mut entries: BTreeMap<String, Value> = BTreeMap::new();
        for (path, (mode, blob)) in &self.files {
            let relative = if dir.is_empty() {
                Some(path.as_str())
            } else {
                path.strip_prefix(&format!("{dir}/"))
            };
            let Some(relative) = relative else { continue };
            match relative.split_once('/') {
                Some((name, _)) => entries.insert(
                    name.to_owned(),
                    json!({"name": name, "mode": 0o040_000, "type": "tree", "oid": "f".repeat(40)}),
                ),
                None => entries.insert(
                    relative.to_owned(),
                    json!({"name": relative, "mode": mode, "type": "blob", "oid": blob["oid"]}),
                ),
            };
        }
        entries.into_values().collect()
    }
}

impl Answers {
    /// The answer of every alias whose expression names `commit`, under
    /// `repository` (the alias `r{g}` its document reads it under).
    fn answer(
        &self,
        commit: &str,
        variables: &serde_json::Map<String, Value>,
        errors: &mut Vec<Value>,
        repository: &str,
    ) -> serde_json::Map<String, Value> {
        let mut objects = serde_json::Map::new();
        for (alias, expression) in variables {
            let Some(expression) = expression.as_str() else {
                continue;
            };
            let file = alias.starts_with('f');
            if !(file || alias.starts_with('d'))
                || !(expression == commit || expression.starts_with(&format!("{commit}:")))
            {
                continue;
            }
            if self.failing.contains(alias) {
                objects.insert(alias.clone(), Value::Null);
                errors.push(json!({"message": "denied", "path": [repository, alias]}));
                continue;
            }
            let at = expression
                .strip_prefix(&format!("{commit}:"))
                .unwrap_or_default();
            let answer = if file {
                self.files
                    .get(at)
                    .map_or(Value::Null, |(_, blob)| blob.clone())
            } else if expression == commit {
                json!({"tree": {"entries": self.entries("")}})
            } else if self.not_trees.iter().any(|folder| folder == at) {
                json!({})
            } else {
                json!({"entries": self.entries(at)})
            };
            objects.insert(alias.clone(), answer);
        }
        objects
    }
}

/// GraphQL for several repositories: `(owner, commit, answers)` each, the
/// group of a document's `r{g}` alias found by its owner variable `o{g}`.
struct Repositories(Vec<(&'static str, &'static str, Answers)>);

impl Respond for Repositories {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).expect("graphql body");
        let variables = body["variables"].as_object().expect("variables");
        let mut data = serde_json::Map::new();
        let mut errors = Vec::new();
        for (name, owner) in variables {
            let Some(group) = name.strip_prefix('o') else {
                continue;
            };
            let Some((_, commit, answers)) = self.0.iter().find(|(known, ..)| owner == known)
            else {
                continue;
            };
            let alias = format!("r{group}");
            let objects = answers.answer(commit, variables, &mut errors, &alias);
            data.insert(alias, Value::Object(objects));
        }
        ResponseTemplate::new(200).set_body_json(json!({"data": data, "errors": errors}))
    }
}

async fn mount(server: &MockServer, answers: Answers, rest: GitRepository) {
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(Repositories(vec![("a", COMMIT, answers)]))
        .mount(server)
        .await;
    rest.mount(server, "a", "b", COMMIT).await;
}

fn context() -> RequestContext {
    with_token("secret", Duration::from_secs(5), 1 << 20)
}

fn at(path: &str) -> ContentRequest {
    ContentRequest {
        owner: "a".into(),
        repo: "b".into(),
        path: path.into(),
        reference: Some(COMMIT.into()),
    }
}

async fn read(provider: &GitHubProvider, path: &str, context: &RequestContext) -> Vec<u8> {
    provider
        .get_file_bytes_following_links(&at(path), context, &|_: &str| false)
        .await
        .expect("read")
        .expect("not withheld")
        .0
        .bytes
        .to_vec()
}

/// Requests other than GraphQL's.
async fn rest_requests(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() != "/api/graphql")
        .count()
}

async fn graphql_requests(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .count()
}

fn paths(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// Reads `files` of `a/b` at [`COMMIT`] ahead, as a search page does.
async fn warm(provider: &GitHubProvider, files: &[String], context: &RequestContext) {
    let group = crate::FilesAtCommit {
        owner: "a",
        repo: "b",
        commit: COMMIT,
        paths: files,
    };
    provider.warm_files_at_commits(&[group], context).await;
}

#[tokio::test]
async fn a_commits_files_are_read_in_one_request_and_then_read_with_nothing_sent() {
    let server = MockServer::start().await;
    let files = [
        ("src/a.rs", "fn a() {}\n"),
        ("src/b.rs", "fn b() {}\r\n"),
        ("README.md", "# α\n"),
    ];
    let mut answers = Answers::default();
    let mut rest = GitRepository::new();
    for (path, text) in files {
        answers = answers.file(path, text);
        rest = rest.file(path, text);
    }
    mount(&server, answers, rest).await;
    let (provider, _) = keeping_provider(&server).await;
    let context = context();
    let names = files.map(|(path, _)| path);
    warm(&provider, &paths(&names), &context).await;
    assert_eq!(graphql_requests(&server).await, 1);
    for (path, text) in files {
        assert_eq!(read(&provider, path, &context).await, text.as_bytes());
        // A read that refuses links reads the same proof.
        let bytes = provider
            .get_file_bytes(&at(path), &context)
            .await
            .expect("read")
            .bytes;
        assert_eq!(bytes.as_ref(), text.as_bytes());
    }
    assert_eq!(
        rest_requests(&server).await,
        0,
        "every read was vouched for"
    );
    // Asked again, nothing is sent: the files already answer.
    warm(&provider, &paths(&names), &context).await;
    assert_eq!(graphql_requests(&server).await, 1);
}

/// One way GitHub's answer cannot vouch for a file: its name and the change
/// to the answer.
type Case = (&'static str, Box<dyn Fn(Answers) -> Answers>);

#[tokio::test]
async fn a_file_the_answer_cannot_vouch_for_is_read_by_rest() {
    let text = "caf\u{20ac}\n";
    // GitHub's text with a character replaced: the same byteSize, other bytes.
    let lossy = "caf\u{fffd}\n";
    assert_eq!(lossy.len(), text.len());
    let cases: [Case; 6] = [
        (
            "a lossy text of the same byteSize",
            Box::new(move |answers| {
                answers.altered("src/x.rs", |blob| blob["text"] = json!(lossy))
            }),
        ),
        (
            "a cut text",
            Box::new(|answers| {
                answers.altered("src/x.rs", |blob| blob["isTruncated"] = json!(true))
            }),
        ),
        (
            "a binary blob",
            Box::new(|answers| {
                answers.altered("src/x.rs", |blob| {
                    blob["isBinary"] = json!(true);
                    blob["text"] = Value::Null;
                })
            }),
        ),
        (
            "a symlink entry",
            Box::new(|answers| answers.mode("src/x.rs", 0o120_000)),
        ),
        (
            "a blob that is not the tree's entry",
            Box::new(|answers| {
                answers.altered("src/x.rs", |blob| blob["oid"] = json!("e".repeat(40)))
            }),
        ),
        (
            "an error naming the file's alias",
            Box::new(|mut answers| {
                answers.failing.push("f0".to_owned());
                answers
            }),
        ),
    ];
    for (case, alter) in cases {
        let server = MockServer::start().await;
        let answers = alter(
            Answers::default()
                .file("src/x.rs", text)
                .file("src/y.rs", "y\n"),
        );
        let rest = GitRepository::new()
            .file("src/x.rs", text)
            .file("src/y.rs", "y\n");
        mount(&server, answers, rest).await;
        let (provider, _) = keeping_provider(&server).await;
        let context = context();
        warm(&provider, &paths(&["src/x.rs", "src/y.rs"]), &context).await;
        assert_eq!(
            read(&provider, "src/x.rs", &context).await,
            text.as_bytes(),
            "{case}"
        );
        assert!(
            rest_requests(&server).await > 0,
            "{case}: x is read by REST"
        );
        let before = rest_requests(&server).await;
        assert_eq!(
            read(&provider, "src/y.rs", &context).await,
            b"y\n",
            "{case}"
        );
        assert_eq!(
            rest_requests(&server).await,
            before,
            "{case}: y stays vouched for"
        );
    }
}

#[tokio::test]
async fn a_folder_that_is_no_tree_vouches_for_none_of_its_files() {
    let server = MockServer::start().await;
    let mut answers = Answers::default().file("docs/a.md", "a\n");
    // GraphQL resolves the folder to a blob (a link to a directory): no entries.
    answers.not_trees.push("docs".to_owned());
    mount(
        &server,
        answers,
        GitRepository::new().file("docs/a.md", "a\n"),
    )
    .await;
    let (provider, _) = keeping_provider(&server).await;
    let context = context();
    warm(&provider, &paths(&["docs/a.md"]), &context).await;
    assert_eq!(read(&provider, "docs/a.md", &context).await, b"a\n");
    assert!(rest_requests(&server).await > 0);
}

#[tokio::test]
async fn without_a_credential_nothing_is_asked_of_graphql() {
    let server = MockServer::start().await;
    mount(
        &server,
        Answers::default().file("a.rs", "a\n"),
        GitRepository::new().file("a.rs", "a\n"),
    )
    .await;
    let (provider, _) = keeping_provider(&server).await;
    let anonymous = RequestContext::with_timeout(Duration::from_secs(5), 1 << 20);
    warm(&provider, &paths(&["a.rs"]), &anonymous).await;
    assert_eq!(graphql_requests(&server).await, 0);
}

/// Files of two repositories share one request: each answered under its
/// repository's alias, each kept for its own repository's reads.
#[tokio::test]
async fn files_of_two_repositories_are_read_in_one_request() {
    const OTHER: &str = "89abcdef0123456789abcdef0123456789abcdef";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(Repositories(vec![
            ("a", COMMIT, Answers::default().file("x.rs", "x\n")),
            ("c", OTHER, Answers::default().file("src/y.rs", "y\n")),
        ]))
        .mount(&server)
        .await;
    let (provider, _) = keeping_provider(&server).await;
    let context = context();
    let (x, y) = (paths(&["x.rs"]), paths(&["src/y.rs"]));
    let groups = [
        crate::FilesAtCommit {
            owner: "a",
            repo: "b",
            commit: COMMIT,
            paths: &x,
        },
        crate::FilesAtCommit {
            owner: "c",
            repo: "d",
            commit: OTHER,
            paths: &y,
        },
    ];
    provider.warm_files_at_commits(&groups, &context).await;
    assert_eq!(graphql_requests(&server).await, 1);
    assert_eq!(read(&provider, "x.rs", &context).await, b"x\n");
    let other = ContentRequest {
        owner: "c".into(),
        repo: "d".into(),
        path: "src/y.rs".into(),
        reference: Some(OTHER.into()),
    };
    let bytes = provider
        .get_file_bytes(&other, &context)
        .await
        .expect("read")
        .bytes;
    assert_eq!(bytes.as_ref(), b"y\n");
    assert_eq!(rest_requests(&server).await, 0);
}
