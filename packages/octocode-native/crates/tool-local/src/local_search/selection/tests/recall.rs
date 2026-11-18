//! A recall oracle for the `.catch(` corpus: octocodecode
//! `packages/mcp/src` (working tree over `cc30009fc02956d5b602a4dfd23687ce0444e53f`, the
//! 15 files holding a `.catch(` hit, copied verbatim under `corpus/mcp/`).
//! 33 hits: 31 discard the error and 2 handle it (`presets/browser/proxy.ts:139`
//! rethrows after dropping its listener, `events.ts:198` notifies the user).
//! The provider's answers are recorded from that ground truth, so a
//! regression in the window, the target, or the verdict mapping changes what
//! is kept. No provider is called.
use super::corpus::judged_over;
use super::*;
use octocode_test_support::json_fixture;
use wiremock::{Request, ResponseTemplate};

const FILES: &[(&str, &str)] = &[
    (
        "client/client.ts",
        include_str!("corpus/mcp/client/client.ts.txt"),
    ),
    (
        "client/oauth/discovery.ts",
        include_str!("corpus/mcp/client/oauth/discovery.ts.txt"),
    ),
    (
        "client/oauth/provider.ts",
        include_str!("corpus/mcp/client/oauth/provider.ts.txt"),
    ),
    (
        "client/oauth/sdk.ts",
        include_str!("corpus/mcp/client/oauth/sdk.ts.txt"),
    ),
    (
        "client/transports/http-body.ts",
        include_str!("corpus/mcp/client/transports/http-body.ts.txt"),
    ),
    (
        "client/transports/streamable-http/http.ts",
        include_str!("corpus/mcp/client/transports/streamable-http/http.ts.txt"),
    ),
    (
        "connections.ts",
        include_str!("corpus/mcp/connections.ts.txt"),
    ),
    ("events.ts", include_str!("corpus/mcp/events.ts.txt")),
    (
        "oauth/auth-provider.ts",
        include_str!("corpus/mcp/oauth/auth-provider.ts.txt"),
    ),
    (
        "oauth/credential-store.ts",
        include_str!("corpus/mcp/oauth/credential-store.ts.txt"),
    ),
    (
        "oauth/sign-in.ts",
        include_str!("corpus/mcp/oauth/sign-in.ts.txt"),
    ),
    (
        "presets/browser/presentation.ts",
        include_str!("corpus/mcp/presets/browser/presentation.ts.txt"),
    ),
    (
        "presets/browser/proxy.ts",
        include_str!("corpus/mcp/presets/browser/proxy.ts.txt"),
    ),
    ("runtime.ts", include_str!("corpus/mcp/runtime.ts.txt")),
    (
        "runtime/client.ts",
        include_str!("corpus/mcp/runtime/client.ts.txt"),
    ),
];

/// The two hits that handle their error, by path suffix and line.
const HANDLED: [(&str, u64); 2] = [("presets/browser/proxy.ts", 139), ("events.ts", 198)];
const HITS: usize = 33;

/// One recorded answer: `discards the error` is the predicate, so a hit that
/// discards it is selected (level 2) and one that handles it is excluded
/// (level 0).
fn recorded_answers(request: &Request) -> ResponseTemplate {
    let body: Value = request.body_json().expect("request");
    let path = body["state"]["path"].as_str().unwrap_or_default();
    let answers = body["questions"]
        .as_object()
        .expect("questions")
        .iter()
        .map(|(id, question)| {
            let label = question["instructions"]["target"].as_str().expect("target");
            let line: u64 = label
                .strip_prefix("line ")
                .and_then(|rest| rest.split(':').next())
                .and_then(|number| number.parse().ok())
                .expect("target line");
            let handled = HANDLED
                .iter()
                .any(|(suffix, at)| path.ends_with(suffix) && *at == line);
            let (level, probabilities) = if handled {
                (0.0, json_fixture(r#"{"0": 1.0, "1": 0.0, "2": 0.0}"#))
            } else {
                (2.0, json_fixture(r#"{"0": 0.0, "1": 0.0, "2": 1.0}"#))
            };
            let legend = question["criteria"]
                .as_array()
                .expect("criteria")
                .iter()
                .enumerate()
                .map(|(i, criterion)| (i.to_string(), criterion.clone()))
                .collect::<serde_json::Map<String, Value>>();
            (
                id.clone(),
                json!({"type":"score","score":level,"confidence":1.0,
                "legend":legend,"probabilities":probabilities}),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    ResponseTemplate::new(200).set_body_json(json!({
        "model":"jev-resolved","answers":answers,
        "usage":{"input_tokens":10,"output_tokens":2}
    }))
}

/// Every `.catch(` line of the corpus: (path, line, trimmed text).
fn corpus_hits() -> Vec<(String, u64, String)> {
    FILES
        .iter()
        .flat_map(|(path, text)| {
            text.lines()
                .enumerate()
                .filter(|(_, line)| line.contains(".catch("))
                .map(|(at, line)| ((*path).to_owned(), at as u64 + 1, line.trim().to_owned()))
        })
        .collect()
}

/// The hit lines a result keeps, by file.
fn kept(result: &LocalSearchResult) -> Vec<(String, u64)> {
    let result = serde_json::to_value(result).expect("result");
    let mut kept = Vec::new();
    for file in result["files"].as_array().into_iter().flatten() {
        for found in file["matches"].as_array().into_iter().flatten() {
            let lines = found["matchedLines"]
                .as_array()
                .map(|lines| lines.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_else(|| vec![found["line"].as_u64().expect("line")]);
            for line in lines {
                kept.push((file["path"].as_str().expect("path").to_owned(), line));
            }
        }
    }
    kept
}

/// The recorded answers keep the 31 hits that discard the error and
/// exclude the 2 that handle it, at every window and in the matchOnly
/// view; identical lines get identical verdicts.
#[test]
fn the_catch_corpus_keeps_the_31_discarding_hits_and_excludes_the_two_handlers() {
    let hits = corpus_hits();
    assert_eq!(hits.len(), HITS, "the corpus is the 33-hit one");
    let handled = hits
        .iter()
        .filter(|(path, line, _)| {
            HANDLED
                .iter()
                .any(|(suffix, at)| path.ends_with(suffix) && at == line)
        })
        .count();
    assert_eq!(handled, HANDLED.len(), "the ground truth names real hits");
    for fields in [
        json_fixture(r#"{"contextLines": {"before": 0, "after": 1}}"#),
        json_fixture(r#"{"contextLines": {"before": 0, "after": 3}}"#),
        json_fixture(r#"{"view": "matchOnly"}"#),
    ] {
        let (result, _) = judged_over(FILES, recorded_answers, fields.clone());
        let receipt = result.selection.as_ref().expect("receipt");
        let (selected, excluded) = (receipt.selected_items, receipt.excluded_items);
        assert_eq!(selected as usize, HITS - HANDLED.len(), "{fields}: kept");
        assert_eq!(excluded as usize, HANDLED.len(), "{fields}: excluded");
        assert_eq!(receipt.unknown_items, 0, "{fields}");
        let kept = kept(&result);
        for (path, line, text) in &hits {
            let ground_truth = !HANDLED
                .iter()
                .any(|(suffix, at)| path.ends_with(suffix) && at == line);
            assert_eq!(
                kept.contains(&(path.clone(), *line)),
                ground_truth,
                "{fields}: {path}:{line} `{text}`"
            );
        }
        // Identical lines are judged identically.
        for (_, _, text) in &hits {
            let verdicts = hits
                .iter()
                .filter(|(_, _, other)| other == text)
                .map(|(path, line, _)| kept.contains(&(path.clone(), *line)))
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(verdicts.len(), 1, "{fields}: `{text}` verdicts differ");
        }
    }
}
