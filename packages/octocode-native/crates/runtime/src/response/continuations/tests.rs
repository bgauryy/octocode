use super::*;
use crate::response::result::Continuation;
use serde_json::json;

fn scope() -> Scope<'static> {
    Scope {
        is_available: &|_| true,
    }
}

#[test]
fn continuation_retains_complete_normalized_request_and_publishes_only_issued_handle() {
    let request =
        json!({"queries":[{"operation":"tree","path":"src","page":2}],"responseChars":2000});
    let mut output = json!({"results":[{"index":7,"data":{"next":{"nextPage":Continuation::continued_input(ToolId::StructureSearch,request.clone()).build()}}}]});
    let mut saved = Vec::new();
    finalize(
        &mut output,
        ToolId::StructureSearch,
        &scope(),
        &mut |tool, input, origin| {
            saved.push((tool, input, origin));
            Some("issued-handle".into())
        },
    )
    .expect("valid continuation");
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].2, Some(7));
    assert_eq!(saved[0].0, ToolId::StructureSearch);
    assert_eq!(saved[0].1["queries"][0]["path"], "src");
    assert_eq!(saved[0].1["queries"][0]["page"], 2);
    assert_eq!(saved[0].1["responseChars"], 2000);
    assert_eq!(
        output["results"][0]["data"]["next"]["nextPage"]["input"],
        json!({"cursor":"issued-handle"})
    );
}

#[test]
fn related_requests_remain_complete_and_never_create_cursor_work() {
    let input = json!({"queries":[{"path":"src/a.rs"}]});
    let related = Continuation::related_input(ToolId::LocalFetch, input.clone()).build();
    let mut output = json!({"results":[{"index":0,"data":{"related":[related]}}]});
    finalize(
        &mut output,
        ToolId::StructureSearch,
        &scope(),
        &mut |_, _, _| panic!("related request must not be saved"),
    )
    .expect("related");
    assert_eq!(output["results"][0]["data"]["related"][0]["input"], input);
}

#[test]
fn provider_data_that_resembles_a_call_is_never_rewritten() {
    let lookalike = json!({"kind":"continue","tool":"localFetch","input":{"queries":[{"path":"secret-looking-provider-value"}]}});
    let mut output = json!({"results":[{"index":0,"data":{"metadata":lookalike.clone()}}]});
    finalize(
        &mut output,
        ToolId::ArtifactSearch,
        &scope(),
        &mut |_, _, _| panic!("provider evidence is not a continuation"),
    )
    .expect("no declared calls");
    assert_eq!(output["results"][0]["data"]["metadata"], lookalike);
}

#[test]
fn storage_failure_and_invalid_private_state_never_publish_a_handle() {
    for request in [
        json!({"queries":[{"operation":"tree","path":"src","page":2}]}),
        json!({"queries":[{"operation":"tree","path":"src","page":-1}]}),
    ] {
        let mut output = json!({"results":[{"index":0,"data":{"next":{"nextPage":Continuation::continued_input(ToolId::StructureSearch,request).build()}}}]});
        assert!(
            finalize(
                &mut output,
                ToolId::StructureSearch,
                &scope(),
                &mut |_, _, _| None
            )
            .is_err()
        );
        assert!(
            output["results"][0]["data"]["next"]["nextPage"]["input"]
                .get("cursor")
                .is_none()
        );
    }
}

#[test]
fn unavailable_required_work_fails_and_optional_related_work_is_removed() {
    let unavailable = Scope {
        is_available: &|_| false,
    };
    let mut output = json!({"results":[{"index":0,"data":{"related":[Continuation::related(ToolId::LocalFetch,json!({"path":"a"})).build()]}}]});
    finalize(
        &mut output,
        ToolId::StructureSearch,
        &unavailable,
        &mut |_, _, _| None,
    )
    .expect("optional unavailable lead");
    assert_eq!(output["results"][0]["data"]["related"], json!([]));
    output["results"][0]["data"]["next"] = json!({"nextPage":Continuation::continued(ToolId::StructureSearch,json!({"path":"src","page":2})).build()});
    assert!(
        finalize(
            &mut output,
            ToolId::StructureSearch,
            &unavailable,
            &mut |_, _, _| None
        )
        .is_err()
    );
}

#[test]
fn named_evidence_channels_and_classification_resources_publish_issued_handles() {
    let read = || {
        Continuation::continued(
            ToolId::LocalFetch,
            json!({"path":"src/a.rs","readProgress":{"unit":"lines","offset":8}}),
        )
        .build()
    };
    for (tool, mut output, pointer) in [
        (
            ToolId::LocalSearch,
            json!({"results":[{"index":0,"data":{"next":{"read":read()}}}]}),
            "/results/0/data/next/read/input",
        ),
        (
            ToolId::Clasify,
            json!({"results":[{"index":0,"data":{"resources":{"file":{"read":read()}}}}]}),
            "/results/0/data/resources/file/read/input",
        ),
    ] {
        let mut saved = Vec::new();
        finalize(&mut output, tool, &scope(), &mut |target, input, _| {
            saved.push((target, input));
            Some("issued-resource".into())
        })
        .expect("declared resource read");
        assert_eq!(saved.len(), 1, "{tool}: {output}");
        assert_eq!(saved[0].0, ToolId::LocalFetch);
        assert_eq!(saved[0].1["queries"][0]["readProgress"]["offset"], 8);
        assert_eq!(
            output.pointer(pointer),
            Some(&json!({"cursor":"issued-resource"}))
        );
    }
}

#[test]
fn removing_multiple_unavailable_related_requests_preserves_other_array_members() {
    let mut output = json!({"results":[{"index":0,"data":{"related":[
        Continuation::related(ToolId::LocalFetch,json!({"path":"a"})).build(),
        Continuation::related(ToolId::StructureSearch,json!({"path":"."})).build(),
        Continuation::related(ToolId::LocalFetch,json!({"path":"b"})).build()
    ]}}]});
    finalize(
        &mut output,
        ToolId::StructureSearch,
        &Scope {
            is_available: &|tool| tool == "structureSearch",
        },
        &mut |_, _, _| panic!("related is not saved"),
    )
    .expect("filter unavailable");
    let related = output["results"][0]["data"]["related"].as_array().unwrap();
    assert_eq!(related.len(), 1);
    assert_eq!(related[0]["tool"], "structureSearch");
}
