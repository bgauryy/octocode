//! localSearch `keyPath`: hits kept to the value at a key path of JSON and
//! TOML documents. A line is kept when a member or element inside the key
//! path starts on it (also inside a one-line object or array), or when it
//! continues that value without starting a member of its own; `*` stands for
//! any one key or index (`contributes.commands[*].command`). The scope keeps
//! whole lines: a kept line that also starts a member outside the key path
//! (a one-line object, a minified document) is disclosed ([`shared_note`]).
//! Every other file is left out unread.
use crate::tools::match_keys;
use octocode_engine::types::LineScope;
use std::path::Path;

/// One step of a key path: a key, or an array index (`[0]`).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    Key(String),
    Index(String),
}

/// The steps of a key path as [`match_keys`] spells one: plain keys joined by
/// `.`, a quoted key as `["…"]`, an index as `[N]`.
fn steps(path: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut rest = path;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('.') {
            rest = after;
            continue;
        }
        if let Some(inner) = rest.strip_prefix('[') {
            // A quoted key is one JSON string: its end is the first unescaped quote.
            if inner.starts_with('"') {
                let mut stream = serde_json::Deserializer::from_str(inner).into_iter::<String>();
                if let Some(Ok(key)) = stream.next() {
                    let after = inner.get(stream.byte_offset()..).unwrap_or_default();
                    steps.push(Step::Key(key));
                    rest = after.strip_prefix(']').unwrap_or(after);
                    continue;
                }
            }
            let (index, after) = inner.split_once(']').unwrap_or((inner, ""));
            steps.push(Step::Index(index.to_owned()));
            rest = after;
            continue;
        }
        let (key, after) = rest.split_at(rest.find(['.', '[']).unwrap_or(rest.len()));
        steps.push(Step::Key(key.to_owned()));
        rest = after;
    }
    steps
}

/// Whether `line` (a hit line's key path) is `scope` or lies inside it.
fn inside(scope: &[Step], line: &[Step]) -> bool {
    let any = |step: &Step| matches!(step, Step::Key(key) | Step::Index(key) if key == "*");
    line.len() >= scope.len()
        && scope
            .iter()
            .zip(line)
            .all(|(want, have)| any(want) || want == have)
}

/// Whether `path` (a member's key path) holds `scope`: a container the
/// scope lies below.
fn holds(path: &[Step], scope: &[Step]) -> bool {
    path.len() < scope.len() && inside(&scope[..path.len()], path)
}

/// The lines of one document a scope keeps, and of them the lines that
/// also hold members outside it.
#[derive(Debug, Default, PartialEq, Eq)]
struct ScopeLines {
    /// 1-based inclusive spans of the kept lines, merged, in line order.
    spans: Vec<[u32; 2]>,
    /// Kept lines where a member or element outside the scope starts too
    /// (a one-line object or array, a minified document): a line is kept
    /// whole, so a hit there may lie outside the scope.
    shared: Vec<u32>,
}

/// How each line stands to a scope, as the document scan tells it.
struct LineJudge<'a> {
    scope: &'a [Step],
    /// The current line: its number, whether it begins inside the scope,
    /// whether a member starts on it, inside the scope or outside it.
    line: Option<(usize, bool, bool, bool, bool)>,
    lines: ScopeLines,
}

impl LineJudge<'_> {
    fn flush(&mut self) {
        let Some((line, begins_inside, started, inside, outside)) = self.line.take() else {
            return;
        };
        // A line is kept when the scope's value starts on it, or when it
        // continues that value without a member of its own.
        if !(inside || (begins_inside && !started)) {
            return;
        }
        let line = octocode_engine::num::saturating_u32(line);
        match self.lines.spans.last_mut() {
            Some([_, end]) if end.saturating_add(1) == line => *end = line,
            _ => self.lines.spans.push([line, line]),
        }
        if outside {
            self.lines.shared.push(line);
        }
    }
}

impl match_keys::LineVisitor for LineJudge<'_> {
    fn line(&mut self, line: usize, container: &str) {
        self.flush();
        let begins_inside = !container.is_empty() && inside(self.scope, &steps(container));
        self.line = Some((line, begins_inside, false, false, false));
    }

    fn start(&mut self, path: &str) -> bool {
        let Some((_, _, started, inside_scope, outside)) = self.line.as_mut() else {
            return false;
        };
        let path = steps(path);
        *started = true;
        if inside(self.scope, &path) {
            *inside_scope = true;
        } else if !holds(&path, self.scope) || matches!(path.last(), Some(Step::Key(_))) {
            // A container above the scope opens here without text of its
            // own (`{`, `[`, an element); a key above it is text outside.
            *outside = true;
        }
        // Both facts known: the rest of the line changes nothing.
        !(*inside_scope && *outside)
    }
}

/// The lines of `text`, a document at `path`, that `scope` keeps: each
/// line a member or element inside the scope starts on (one inside a
/// one-line object or array among them), and each line inside its value
/// that starts no member.
fn scope_lines(scope: &[Step], path: &Path, text: &str) -> ScopeLines {
    // Each line a hit can be on; none past a final line end.
    let lines = (1..=text.lines().count()).collect::<Vec<_>>();
    let mut judge = LineJudge {
        scope,
        line: None,
        lines: ScopeLines::default(),
    };
    match_keys::visit_lines(path, text, &lines, &mut judge);
    judge.flush();
    judge.lines
}

/// The disclosure of the hits in `files` (paths under `output_root`) that lie
/// on lines `key_path` keeps but shares with members outside it: their
/// lines are kept whole, so they may lie outside the key path. `None` when
/// no hit does. Each file is read once the read policy passes it.
pub(super) fn shared_note(
    key_path: &str,
    (paths, output_root): (&crate::policy::path::PathPolicy, &Path),
    files: &[octocode_engine::types::TextSearchFile],
) -> Option<String> {
    let scope = steps(key_path);
    let (mut hits, mut lines, mut holders) = (0usize, 0usize, 0usize);
    for file in files {
        let source = output_root.join(&file.path);
        if !match_keys::is_keyed_path(&source) || paths.validate_read(&source).is_err() {
            continue;
        }
        let Some(text) = document(&source) else {
            continue;
        };
        let shared = scope_lines(&scope, &source, &text).shared;
        let on_shared = |line: u32, last: Option<u32>| {
            let at = shared.partition_point(|shared| *shared < line);
            shared
                .get(at)
                .is_some_and(|shared| *shared <= last.unwrap_or(line))
        };
        let held = file
            .matches
            .iter()
            .filter(|hit| on_shared(hit.line, hit.last_line))
            .collect::<Vec<_>>();
        if held.is_empty() {
            continue;
        }
        hits += held.len();
        let mut held_lines = held.iter().map(|hit| hit.line).collect::<Vec<_>>();
        held_lines.sort_unstable();
        held_lines.dedup();
        lines += held_lines.len();
        holders += 1;
    }
    (hits > 0).then(|| {
        format!(
            "keyPath {key_path}: {hits} hit(s) on {lines} line(s) in {holders} file(s) share their line with members outside it (a one-line object or array, a minified document); a line is kept whole, so those hits may lie outside {key_path}."
        )
    })
}

/// The text of the document at `path`, read as far as a search opens a
/// file (a larger one is never searched); `None` when it cannot be read.
fn document(path: &Path) -> Option<String> {
    let ceiling = octocode_engine::search::text_search::DEFAULT_MAX_LINE_SEARCH_FILE_BYTES;
    octocode_engine::text::read_regular(path, ceiling)
        .ok()
        .map(|source| String::from_utf8_lossy(&source.bytes).into_owned())
}

/// The engine scope of `keyPath`: JSON and TOML files only, each kept to
/// the lines of the value at the key path.
pub(super) fn line_scope(key_path: &str) -> LineScope {
    let scope = std::sync::Arc::new(steps(key_path));
    LineScope {
        holds: std::sync::Arc::new(match_keys::is_keyed_path),
        spans: std::sync::Arc::new(move |path: &Path| {
            let text = document(path).unwrap_or_default();
            std::sync::Arc::new(scope_lines(&scope, path, &text).spans)
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKAGE: &str = r#"{
  "name": "ext",
  "activationEvents": [
    "onLanguage:json",
    "onCommand:ext.run"
  ],
  "contributes": {
    "commands": [
      { "command": "ext.run", "title": "onLanguage" }
    ],
    "configuration": { "ext.on": { "default": "onCommand" } }
  }
}
"#;

    /// The kept lines of `scope` in `text` (a document at `name`), each line
    /// once, and the kept lines it shares with members outside it.
    fn kept(name: &str, text: &str, scope: &str) -> (Vec<u32>, Vec<u32>) {
        let lines = scope_lines(&steps(scope), Path::new(name), text);
        let kept = lines
            .spans
            .iter()
            .flat_map(|[start, end]| *start..=*end)
            .collect();
        (kept, lines.shared)
    }

    /// Each scope keeps the lines its value is on, written out by hand per
    /// fixture: multi-line values, a one-line object or array inside a
    /// pretty-printed document, a minified document and TOML inline values.
    /// A kept line that holds members outside the scope is named shared.
    #[test]
    fn a_scope_keeps_the_lines_its_value_is_on() {
        const ONE_LINE: &str = r#"{
  "activationEvents": ["onLanguage:json", "onCommand:ext.run"],
  "main": "./out/ext.js"
}
"#;
        const MINIFIED: &str =
            r#"{"name":"ext","activationEvents":["onLanguage:json"],"main":"x"}"#;
        const CARGO: &str = r#"[package]
name = "x"

[dependencies]
serde = { version = "1", features = ["derive"] }
anyhow = "1"
"#;
        let none: Vec<u32> = Vec::new();
        for (name, text, scope, lines, shared) in [
            (
                "package.json",
                PACKAGE,
                "activationEvents",
                vec![3, 4, 5, 6],
                none.clone(),
            ),
            (
                "package.json",
                PACKAGE,
                "activationEvents[1]",
                vec![5],
                none.clone(),
            ),
            (
                "package.json",
                PACKAGE,
                "contributes.commands",
                vec![8, 9, 10],
                none.clone(),
            ),
            (
                "package.json",
                PACKAGE,
                "contributes.commands[*].title",
                vec![9],
                vec![9],
            ),
            (
                "package.json",
                PACKAGE,
                "contributes.commands[0].command",
                vec![9],
                vec![9],
            ),
            (
                "package.json",
                PACKAGE,
                "contributes.*",
                vec![8, 9, 10, 11],
                none.clone(),
            ),
            (
                "package.json",
                PACKAGE,
                r#"contributes.configuration["ext.on"]"#,
                vec![11],
                vec![11],
            ),
            (
                "package.json",
                PACKAGE,
                r#"contributes.configuration["ext.on"].default"#,
                vec![11],
                vec![11],
            ),
            (
                "package.json",
                PACKAGE,
                "missing",
                none.clone(),
                none.clone(),
            ),
            (
                "package.json",
                ONE_LINE,
                "activationEvents",
                vec![2],
                none.clone(),
            ),
            (
                "package.json",
                ONE_LINE,
                "activationEvents[*]",
                vec![2],
                vec![2],
            ),
            (
                "package.json",
                ONE_LINE,
                "activationEvents[1]",
                vec![2],
                vec![2],
            ),
            ("package.json", ONE_LINE, "main", vec![3], none.clone()),
            (
                "package.json",
                MINIFIED,
                "activationEvents",
                vec![1],
                vec![1],
            ),
            ("package.json", MINIFIED, "name", vec![1], vec![1]),
            (
                "package.json",
                MINIFIED,
                "missing",
                none.clone(),
                none.clone(),
            ),
            (
                "Cargo.toml",
                CARGO,
                "dependencies",
                vec![4, 5, 6],
                none.clone(),
            ),
            (
                "Cargo.toml",
                CARGO,
                "dependencies.serde",
                vec![5],
                none.clone(),
            ),
            (
                "Cargo.toml",
                CARGO,
                "dependencies.serde.version",
                vec![5],
                vec![5],
            ),
            (
                "Cargo.toml",
                CARGO,
                "dependencies.serde.features[0]",
                vec![5],
                vec![5],
            ),
            (
                "Cargo.toml",
                CARGO,
                "dependencies.anyhow",
                vec![6],
                none.clone(),
            ),
        ] {
            assert_eq!(kept(name, text, scope), (lines, shared), "{name} {scope}");
        }
    }

    /// Key paths read back into the steps they were spelled from: plain keys,
    /// quoted keys holding dots, brackets and quotes, and indices.
    #[test]
    fn key_paths_read_back_into_their_steps() {
        for (path, expected) in [
            ("a.b", vec![Step::Key("a".into()), Step::Key("b".into())]),
            (
                r#"exports["./node"].import"#,
                vec![
                    Step::Key("exports".into()),
                    Step::Key("./node".into()),
                    Step::Key("import".into()),
                ],
            ),
            (
                r#"a["x]\"y"][2]"#,
                vec![
                    Step::Key("a".into()),
                    Step::Key("x]\"y".into()),
                    Step::Index("2".into()),
                ],
            ),
            (
                "[*].name",
                vec![Step::Index("*".into()), Step::Key("name".into())],
            ),
        ] {
            assert_eq!(steps(path), expected, "{path}");
        }
    }

    /// Only JSON and TOML files can hold a kept line.
    #[test]
    fn only_keyed_files_are_held() {
        let scope = line_scope("dependencies");
        for (path, keyed) in [
            ("Cargo.toml", true),
            ("package.json", true),
            ("a.rs", false),
        ] {
            assert_eq!((scope.holds)(Path::new(path)), keyed, "{path}");
        }
    }
}
