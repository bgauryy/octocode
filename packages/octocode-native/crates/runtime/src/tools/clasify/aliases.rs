//! The unified clasify input (flat resources: `value`, or `tool`+`query`;
//! questions: `type`+`ask` with `labels`/`known`) and the nested provider
//! shape the runtime executes (`context`, `questionType`/`target`,
//! `type`/`instructions`/`criteria`). The contract accepts both; validated
//! queries are mapped onto the nested shape before execution, so both reach
//! the provider as the same payload. Continuations publish the unified shape.
use super::{is_candidate_search_tool, is_file_read_tool};
use serde_json::{Map, Value, json};

/// Unified research `type` → nested `questionType`.
const RESEARCH: [(&str, &str); 5] = [
    ("locate", "locate"),
    ("sufficient", "sufficient"),
    ("relevant", "contribution"),
    ("supports", "supportsClaim"),
    ("adds", "addsEvidence"),
];
/// Unified custom `type` → nested provider `type`.
const CUSTOM: [(&str, &str); 3] = [("yesno", "noul"), ("choice", "choice"), ("score", "score")];

/// File-read query fields that already select what to read; without one, a
/// flat file resource reads the whole file (the nested form's `fullContent`).
const READ_SELECTORS: [&str; 11] = [
    "fullContent",
    "startLine",
    "endLine",
    "ranges",
    "block",
    "matchString",
    "charOffset",
    "charLength",
    "offset",
    "chunkSize",
    "minify",
];

fn lookup(table: &[(&'static str, &'static str)], value: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|(unified, _)| *unified == value)
        .map(|(_, nested)| *nested)
}

fn reverse(table: &[(&'static str, &'static str)], value: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|(_, nested)| *nested == value)
        .map(|(unified, _)| *unified)
}

fn is_locate(question: &Value) -> bool {
    question.get("type").and_then(Value::as_str) == Some("locate")
        || question.get("questionType").and_then(Value::as_str) == Some("locate")
}

/// Map one validated matrix onto the nested provider shape in place. Nested
/// resources and questions pass through unchanged.
pub(crate) fn canonicalize(query: &mut Value) {
    let locate = query["questions"]
        .as_array()
        .is_some_and(|questions| questions.iter().any(is_locate));
    if let Some(resources) = query.get_mut("resources").and_then(Value::as_array_mut) {
        for resource in resources {
            canonical_resource(resource, locate);
        }
    }
    if let Some(questions) = query.get_mut("questions").and_then(Value::as_array_mut) {
        for question in questions {
            canonical_question(question);
        }
    }
}

fn canonical_resource(resource: &mut Value, locate: bool) {
    let Some(fields) = resource.as_object_mut() else {
        return;
    };
    if fields.contains_key("context") {
        return;
    }
    if let Some(value) = fields.remove("value") {
        fields.insert("context".into(), json!({"value": value}));
        return;
    }
    let (Some(tool), Some(mut query)) = (fields.remove("tool"), fields.remove("query")) else {
        return;
    };
    let name = tool.as_str().unwrap_or_default().to_owned();
    if is_file_read_tool(&name)
        && let Some(query) = query.as_object_mut()
        && !READ_SELECTORS.iter().any(|key| query.contains_key(*key))
    {
        query.insert("fullContent".into(), Value::Bool(true));
    }
    let mut context = Map::new();
    context.insert("tool".into(), tool);
    context.insert("query".into(), query);
    match fields.remove("candidateEvidence") {
        Some(evidence) => {
            context.insert("candidateEvidence".into(), evidence);
        }
        // Locate needs source lines: a flat search resource reads fileChunks.
        None if locate && is_candidate_search_tool(&name) => {
            context.insert(
                "candidateEvidence".into(),
                json!(super::CandidateEvidence::FileChunks.to_string()),
            );
        }
        None => {}
    }
    fields.insert("context".into(), Value::Object(context));
}

fn canonical_question(question: &mut Value) {
    let Some(fields) = question.as_object_mut() else {
        return;
    };
    let Some(ask) = fields.remove("ask") else {
        return;
    };
    let kind = fields
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if let Some(nested) = lookup(&RESEARCH, &kind) {
        fields.remove("type");
        fields.insert("questionType".into(), json!(nested));
        fields.insert("target".into(), ask);
        if let Some(known) = fields.remove("known") {
            fields.insert("knownEvidence".into(), known);
        }
    } else if let Some(nested) = lookup(&CUSTOM, &kind) {
        fields.insert("type".into(), json!(nested));
        fields.insert("instructions".into(), ask);
        if let Some(labels) = fields.remove("labels") {
            fields.insert("criteria".into(), labels);
        }
    } else {
        // Validation admits no other unified type; keep the input visible.
        fields.insert("ask".into(), ask);
    }
}

/// The unified form of one nested resource (continuations): the inverse of
/// [`canonicalize`], omitting the default capture cap and implied fields.
pub(crate) fn unified_resource(resource: &Value, locate: bool, default_max_chars: u64) -> Value {
    let Some(fields) = resource.as_object() else {
        return resource.clone();
    };
    let Some(context) = fields.get("context").and_then(Value::as_object) else {
        return resource.clone();
    };
    let mut out = Map::new();
    if let Some(id) = fields.get("id") {
        out.insert("id".into(), id.clone());
    }
    if let Some(value) = context.get("value") {
        out.insert("value".into(), value.clone());
        return Value::Object(out);
    }
    for key in ["tool", "query"] {
        if let Some(value) = context.get(key) {
            out.insert(key.into(), value.clone());
        }
    }
    let tool = context
        .get("tool")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if let Some(evidence) = context.get("candidateEvidence") {
        let implied =
            locate && is_candidate_search_tool(tool) && evidence.as_str() == Some("fileChunks");
        if !implied {
            out.insert("candidateEvidence".into(), evidence.clone());
        }
    }
    if let Some(max) = fields.get("maxChars")
        && max.as_u64() != Some(default_max_chars)
    {
        out.insert("maxChars".into(), max.clone());
    }
    if let Some(prefilter) = fields.get("prefilter") {
        out.insert("prefilter".into(), prefilter.clone());
    }
    Value::Object(out)
}

/// The unified form of one nested question (continuations).
pub(crate) fn unified_question(question: &Value) -> Value {
    let Some(fields) = question.as_object() else {
        return question.clone();
    };
    let mut out = Map::new();
    if let Some(id) = fields.get("id") {
        out.insert("id".into(), id.clone());
    }
    if let Some(kind) = fields.get("questionType").and_then(Value::as_str)
        && let Some(unified) = reverse(&RESEARCH, kind)
    {
        out.insert("type".into(), json!(unified));
        out.insert(
            "ask".into(),
            fields.get("target").cloned().unwrap_or(Value::Null),
        );
        if let Some(known) = fields.get("knownEvidence") {
            out.insert("known".into(), known.clone());
        }
        return Value::Object(out);
    }
    if let Some(kind) = fields.get("type").and_then(Value::as_str)
        && let Some(unified) = reverse(&CUSTOM, kind)
        && let Some(instructions) = fields.get("instructions")
    {
        out.insert("type".into(), json!(unified));
        out.insert("ask".into(), instructions.clone());
        if let Some(criteria) = fields.get("criteria").filter(|value| !value.is_null()) {
            out.insert("labels".into(), criteria.clone());
        }
        return Value::Object(out);
    }
    question.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matrix(resources: Value, questions: Value) -> Value {
        json!({"mainGoal":"g","reasoning":"r","resources":resources,"questions":questions})
    }

    #[test]
    fn unified_matrix_maps_onto_the_nested_provider_shape() {
        let mut query = matrix(
            json!([
                {"id":"f","tool":"localFetch","query":{"path":"/r/a.c"},"prefilter":["cron"]},
                {"id":"s","tool":"localSearch","query":{"path":"/r","searchText":"x"}},
                {"id":"h","value":{"seen":true}},
                {"id":"g","tool":"ghGetFileContent","query":{"owner":"o","repo":"r","path":"a","startLine":1,"endLine":9},"maxChars":100}
            ]),
            json!([
                {"id":"t","type":"locate","ask":"timer"},
                {"type":"relevant","ask":"timer"},
                {"type":"supports","ask":"claim"},
                {"type":"sufficient","ask":"answer"},
                {"type":"adds","ask":"timer","known":["a"]},
                {"type":"yesno","ask":"test?","labels":{"true":"t","false":null}},
                {"type":"choice","ask":{"task":"kind"},"labels":{"a":null,"b":"b"}},
                {"type":"score","ask":"risk","labels":["low","high"]}
            ]),
        );
        canonicalize(&mut query);
        assert_eq!(
            query["resources"],
            json!([
                {"id":"f","context":{"tool":"localFetch","query":{"path":"/r/a.c","fullContent":true}},"prefilter":["cron"]},
                {"id":"s","context":{"tool":"localSearch","query":{"path":"/r","searchText":"x"},"candidateEvidence":"fileChunks"}},
                {"id":"h","context":{"value":{"seen":true}}},
                {"id":"g","context":{"tool":"ghGetFileContent","query":{"owner":"o","repo":"r","path":"a","startLine":1,"endLine":9}},"maxChars":100}
            ])
        );
        assert_eq!(
            query["questions"],
            json!([
                {"id":"t","questionType":"locate","target":"timer"},
                {"questionType":"contribution","target":"timer"},
                {"questionType":"supportsClaim","target":"claim"},
                {"questionType":"sufficient","target":"answer"},
                {"questionType":"addsEvidence","target":"timer","knownEvidence":["a"]},
                {"type":"noul","instructions":"test?","criteria":{"true":"t","false":null}},
                {"type":"choice","instructions":{"task":"kind"},"criteria":{"a":null,"b":"b"}},
                {"type":"score","instructions":"risk","criteria":["low","high"]}
            ])
        );
    }

    #[test]
    fn selected_file_reads_do_not_become_whole_file_reads() {
        for read in [
            json!({"path":"/r/a.c","ranges":["10-20","40-45"]}),
            json!({"path":"/r/a.c","ranges":["10-20"],"block":true}),
        ] {
            let mut query = matrix(
                json!([{"id":"f","tool":"localFetch","query":read.clone()}]),
                json!([{"type":"locate","ask":"timer"}]),
            );
            canonicalize(&mut query);
            assert_eq!(query["resources"][0]["context"]["query"], read);
            let prepared = crate::contracts::prepare_many_and_validate(
                "clasify",
                matrix(
                    json!([{"id":"f","tool":"localFetch","query":read}]),
                    json!([{"type":"locate","ask":"timer"}]),
                ),
                crate::contracts::PrepareOptions::default(),
            )
            .expect("a ranged flat read validates");
            let mut nested = prepared[0].clone();
            canonicalize(&mut nested);
            let mut read = nested["resources"][0]["context"]["query"].clone();
            read["mainGoal"] = json!("g");
            read["reasoning"] = json!("r");
            crate::contracts::prepare_many_and_validate(
                "localFetch",
                read,
                crate::contracts::PrepareOptions::default(),
            )
            .expect("the canonical read keeps its selector alone");
        }
    }

    #[test]
    fn nested_inputs_are_unchanged_and_screens_keep_search_snippets() {
        let nested = matrix(
            json!([{"id":"f","context":{"tool":"localSearch","query":{"path":"/r","searchText":"x"}}}]),
            json!([{"questionType":"contribution","target":"x"},{"type":"noul","instructions":"y"}]),
        );
        let mut mapped = nested.clone();
        canonicalize(&mut mapped);
        assert_eq!(mapped, nested);
        let mut screen = matrix(
            json!([{"tool":"localSearch","query":{"path":"/r","searchText":"x"}}]),
            json!([{"type":"relevant","ask":"x"}]),
        );
        canonicalize(&mut screen);
        assert!(
            screen["resources"][0]["context"]
                .get("candidateEvidence")
                .is_none()
        );
    }

    #[test]
    fn continuations_round_trip_through_the_unified_shape() {
        let resources = json!([
            {"id":"f","context":{"tool":"localFetch","query":{"path":"/r/a.c","startLine":9,"endLine":20}},"prefilter":["cron"],"maxChars":80000},
            {"id":"s","context":{"tool":"localSearch","query":{"path":"/r","searchText":"x"},"candidateEvidence":"fileChunks"},"maxChars":500},
            {"id":"h","context":{"value":"v"}}
        ]);
        let unified = resources
            .as_array()
            .unwrap()
            .iter()
            .map(|resource| unified_resource(resource, true, 80_000))
            .collect::<Vec<_>>();
        assert_eq!(
            Value::Array(unified.clone()),
            json!([
                {"id":"f","tool":"localFetch","query":{"path":"/r/a.c","startLine":9,"endLine":20},"prefilter":["cron"]},
                {"id":"s","tool":"localSearch","query":{"path":"/r","searchText":"x"},"maxChars":500},
                {"id":"h","value":"v"}
            ])
        );
        let questions = json!([
            {"id":"t","questionType":"locate","target":"timer"},
            {"id":"a","questionType":"addsEvidence","target":"x","knownEvidence":"k"},
            {"id":"n","type":"noul","instructions":"y"},
            {"id":"c","type":"choice","instructions":"k","criteria":{"a":null,"b":null}}
        ]);
        let unified_questions = questions
            .as_array()
            .unwrap()
            .iter()
            .map(unified_question)
            .collect::<Vec<_>>();
        let mut round_trip = matrix(Value::Array(unified), Value::Array(unified_questions));
        canonicalize(&mut round_trip);
        assert_eq!(round_trip["questions"], questions);
        assert_eq!(
            round_trip["resources"][1]["context"]["candidateEvidence"],
            "fileChunks"
        );
        assert_eq!(round_trip["resources"][0], {
            let mut first = resources[0].clone();
            first.as_object_mut().unwrap().remove("maxChars");
            first
        });
    }
}
