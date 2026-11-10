//! A classified provider page that leaves units out says what it kept, and an
//! empty one that it is no absence, as every classified reader does; the
//! units it left unknown are one `next.unfilteredUnknown` call away.
use super::*;

/// Excludes every candidate.
struct NoneJudge;
impl SelectionJudge for NoneJudge {
    fn identity(&self) -> Value {
        json!({"test":"none"})
    }
    fn select(&self, call: JudgeCall<'_>) -> Judgments {
        Ok(call
            .admitted
            .iter()
            .map(|&ordinal| (ordinal, Decision::Excluded))
            .collect())
    }
}

/// Keeps `fix`, is unsure of `docs`, excludes the rest.
struct UnsureJudge;
impl SelectionJudge for UnsureJudge {
    fn identity(&self) -> Value {
        json!({"test":"unsure"})
    }
    fn select(&self, call: JudgeCall<'_>) -> Judgments {
        Ok(call
            .admitted
            .iter()
            .map(|&ordinal| {
                let state = call.candidates[ordinal]
                    .state
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_default();
                let decision = if state.contains("fix") {
                    Decision::Selected
                } else if state.contains("docs") {
                    Decision::Unknown
                } else {
                    Decision::Excluded
                };
                (ordinal, decision)
            })
            .collect())
    }
}

fn page(tool: ToolId, judge: impl SelectionJudge + 'static, query: &Value, data: Value) -> Value {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = runtime(judge, &executor);
    let mut structured = json!({"results":[{"index":0,"data":data}]});
    let mut response_query = query.clone();
    apply(tool, &mut structured, query, &mut response_query, &runtime);
    structured["results"][0]["data"].take()
}

fn structure() -> Value {
    json!({"files":[{"dir":"a","files":["fix.rs (1)","other.rs (2)"]},{"dir":"b","files":["plain.rs (3)"]}]})
}

/// The receipt states the kept and total counts; a page that shows all it
/// kept, with nothing to judge, unknown or hidden, adds no warning that
/// restates them (declared intent of B263: counts once).
#[test]
fn a_page_showing_all_it_kept_states_the_counts_once_in_the_receipt() {
    let history = page(
        ToolId::GhSearchHistory,
        FixJudge,
        &query(),
        json!({"commits":commits()}),
    );
    let listing = page(
        ToolId::StructureSearch,
        FixJudge,
        &json!({"path":".","classification":{"predicates":["Is it a fix?"]}}),
        structure(),
    );
    for data in [history, listing] {
        let receipt = &data["selection"];
        let kept = receipt["selectedItems"].as_i64().expect("selectedItems");
        let total = receipt["totalItems"].as_i64().expect("totalItems");
        assert!(0 < kept && kept < total, "{data}");
        assert!(
            !data["warnings"].to_string().contains("classification kept"),
            "{data}"
        );
    }
}

#[test]
fn a_page_that_keeps_nothing_says_an_empty_result_is_not_absence() {
    let history = page(
        ToolId::GhSearchHistory,
        NoneJudge,
        &query(),
        json!({"commits":commits()}),
    );
    let listing = page(
        ToolId::StructureSearch,
        NoneJudge,
        &json!({"path":".","classification":{"predicates":["Is it a fix?"]}}),
        structure(),
    );
    let code = page(
        ToolId::GhSearchCode,
        NoneJudge,
        &json!({"owner":"o","repo":"r","keywords":["x"],"classification":{"predicates":["Is it a fix?"]}}),
        json!({"files":[{"path":"a.rs","matches":[{"line":1,"value":"fix it"}]}]}),
    );
    for data in [history, listing, code] {
        let warnings = data["warnings"].to_string();
        assert!(warnings.contains("classification kept 0 of"), "{data}");
        assert!(
            warnings.contains("An empty classified result is not absence"),
            "{data}"
        );
        assert!(data["next"].get("unfiltered").is_some(), "{data}");
    }
}

#[test]
fn unknown_rows_are_one_unfiltered_unknown_call_away() {
    let query = query();
    let data = page(
        ToolId::GhSearchHistory,
        UnsureJudge,
        &query,
        json!({"commits":commits()}),
    );
    assert!(
        data["warnings"]
            .to_string()
            .contains("none is left to judge, and 1 hit stays unknown"),
        "{data}"
    );
    let call = &data["next"]["unfilteredUnknown"];
    assert_eq!(call["tool"], "ghSearchHistory", "{data}");
    let replay = call["input"]["queries"][0].clone();
    assert_eq!(replay["keywords"], query["keywords"]);
    assert!(replay["selection"].is_string(), "{replay}");

    // The call lists exactly the unknown row, unfiltered and unbilled: no
    // receipt of its own, no further lead, and it says what it lists.
    let view = page(
        ToolId::GhSearchHistory,
        UnsureJudge,
        &replay,
        json!({"commits":commits()}),
    );
    let rows = view["commits"].as_array().expect("commits");
    assert_eq!(rows.len(), 1, "{view}");
    assert_eq!(rows[0]["messageHeadline"], "docs: intro");
    assert!(view.get("selection").is_none(), "{view}");
    for lead in ["unfiltered", "unfilteredUnknown", "nextCandidatePage"] {
        assert!(view["next"].get(lead).is_none(), "{lead}: {view}");
    }
    assert!(
        view["warnings"]
            .to_string()
            .contains("Lists only the 1 hit the selection's walk left undecided"),
        "{view}"
    );
}
