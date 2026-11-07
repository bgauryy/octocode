//! What a classified matchString read sends the judge for each hit.
use super::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::selection::Judgments;
use crate::tools::source_view::SourceFacts;
use serde_json::Value;
use std::path::Path;

/// Records every admitted state and answers nothing.
struct Capture(std::sync::Mutex<Vec<Value>>);

impl SelectionJudge for Capture {
    fn identity(&self) -> Value {
        json!({"test":"capture"})
    }
    fn select(&self, call: JudgeCall<'_>) -> Judgments {
        self.0.lock().expect("lock").extend(
            call.admitted
                .iter()
                .filter_map(|&i| call.candidates[i].state.clone()),
        );
        Ok(Vec::new())
    }
}

/// The states a classified read of `source` at `path` sends.
fn states(path: &str, source: &str, fields: Value) -> Vec<Value> {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let judge = Arc::new(Capture(std::sync::Mutex::new(Vec::new())));
    let runtime = Runtime {
        judge: judge.clone(),
        ..selection_fake::runtime(
            WordJudge { fail: None },
            executor.handle().clone(),
            Duration::from_secs(5),
        )
    };
    let mut query =
        json!({"path":path, "classification":{"predicates":["Is this a runtime dependency?"]}});
    for (key, value) in fields.as_object().into_iter().flatten() {
        query[key] = value.clone();
    }
    let query: LocalFetchQuery = serde_json::from_value(query).expect("query");
    crate::tools::source_view::process_fetched_content_selected(
        &query,
        source.as_bytes(),
        Path::new(path),
        path,
        &SourceFacts::default(),
        &ContentSecurity::new(),
        &NeverCancel,
        Some(&runtime),
    );
    judge.0.lock().expect("lock").clone()
}

/// bgauryy/octocode@24ada10 `packages/octocode-mcp/package.json` in shape:
/// dependencies on lines 9-11, devDependencies from 13.
const PACKAGE_JSON: &str = r#"{
  "name": "octocode-mcp",
  "version": "1.0.0",
  "type": "module",
  "main": "dist/index.js",
  "license": "MIT",
  "scripts": {},
  "dependencies": {
    "a-dep": "^1.0.0",
    "b-dep": "^1.0.0",
    "c-dep": "^1.0.0"
  },
  "devDependencies": {
    "d-dev": "^1.0.0",
    "x": "^1.0.0"
  }
}
"#;

/// A hit whose window cannot show the section it sits in still names it:
/// with no context, `"x"` at line 15 is a devDependency, and its target says
/// so instead of leaving the judge one bare line.
#[test]
fn a_hit_names_the_key_path_of_its_line() {
    let seen = states(
        "package.json",
        PACKAGE_JSON,
        json!({"matchString":["\"x\""],"contextLines":{"before":0,"after":0}}),
    );
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0]["target"]["line"], 15);
    let key = seen[0]["target"]["key"].as_str().unwrap_or_default();
    assert!(key.starts_with("devDependencies"), "{seen:?}");
}

/// A hit names the declaration it sits in, as localSearch's do.
#[test]
fn a_hit_names_its_enclosing_declaration() {
    let seen = states(
        "retry.rs",
        "fn production() {\n    needle();\n}\n\nfn checks_it() {\n    needle();\n}\n",
        json!({"matchString":["needle"],"contextLines":{"before":0,"after":0}}),
    );
    let names = seen
        .iter()
        .map(|state| state["target"]["enclosing"]["symbolName"].clone())
        .collect::<Vec<_>>();
    assert_eq!(names, [json!("production"), json!("checks_it")], "{seen:?}");
}
