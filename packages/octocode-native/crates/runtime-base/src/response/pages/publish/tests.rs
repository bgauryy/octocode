use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

#[test]
fn a_complete_first_page_states_no_block_and_every_other_page_does() {
    let mut output = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "pagination": {
                            "unit": "items",
                            "offset": 0,
                            "length": 3,
                            "total": 3,
                            "hasMore": false
                        },
                        "files": [
                            {
                                "path": "a",
                                "pagination": {
                                    "unit": "lines",
                                    "offset": 0,
                                    "length": 2,
                                    "hasMore": false
                                }
                            }
                        ]
                    }
                },
                {
                    "index": 1,
                    "data": {
                        "pagination": {
                            "unit": "items",
                            "offset": 9,
                            "length": 3,
                            "total": 12,
                            "hasMore": false
                        }
                    }
                },
                {
                    "index": 2,
                    "data": {
                        "pagination": {"unit": "items", "offset": 0, "length": 9, "hasMore": true}
                    }
                },
                {
                    "index": 3,
                    "data": {
                        "pagination": {
                            "unit": "lines",
                            "offset": 4,
                            "length": 2,
                            "hasMore": false
                        }
                    }
                }
            ]
        }"#,
    );
    publish_rows(&mut output);
    let rows = output["results"].as_array().expect("rows");
    assert_eq!(
        rows[0]["data"],
        json_fixture(r#"{"files": [{"path": "a"}]}"#)
    );
    assert_eq!(
        rows[1]["data"]["pagination"],
        json_fixture(r#"{"offset": 9, "total": 12, "hasMore": false}"#)
    );
    assert_eq!(rows[2]["data"]["pagination"]["hasMore"], true);
    assert!(rows[2]["data"]["pagination"].get("unit").is_none());
    // A text window keeps its unit and length: they are not the rows listed.
    assert_eq!(
        rows[3]["data"]["pagination"],
        json_fixture(
            r#"{
                "unit": "lines",
                "offset": 4,
                "length": 2,
                "hasMore": false
            }"#
        )
    );
}

#[test]
fn a_partial_page_states_it_once() {
    let mut output = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "isPartial": true,
                        "partialReasons": ["providerResultCap"],
                        "coverage": {"state": "partial"}
                    }
                },
                {
                    "index": 1,
                    "data": {
                        "isPartial": true,
                        "pagination": {
                            "offset": 0,
                            "total": 1000,
                            "countScope": "capped",
                            "hasMore": true
                        }
                    }
                },
                {
                    "index": 2,
                    "data": {
                        "isPartial": true,
                        "coverage": {"state": "partial", "reason": "x"}
                    }
                }
            ]
        }"#,
    );
    publish_rows(&mut output);
    let rows = output["results"].as_array().expect("rows");
    assert_eq!(
        rows[0]["data"],
        json_fixture(r#"{"partialReasons": ["providerResultCap"]}"#)
    );
    assert!(rows[1]["data"].get("isPartial").is_none(), "{}", rows[1]);
    // No other statement: isPartial stays, the state beside it goes.
    assert_eq!(
        rows[2]["data"],
        json_fixture(r#"{"isPartial": true, "coverage": {"reason": "x"}}"#)
    );
}

#[test]
fn a_listing_summary_is_stated_only_on_a_paged_or_partial_page() {
    let counts = json_fixture(r#"{"fileCount": 2, "dirCount": 1}"#);
    let mut output = json!({"results":[
        {"index":0,"data":{"files":[],"summary":counts}},
        {"index":1,"data":{"files":[],"summary":counts,
            "pagination":{"offset":0,"total":9,"hasMore":true}}},
        {"index":2,"data":{"files":[],"summary":counts,"partialReasons":["filesUnevaluated"]}},
    ]});
    publish_rows(&mut output);
    let rows = output["results"].as_array().expect("rows");
    assert!(rows[0]["data"].get("summary").is_none(), "{}", rows[0]);
    assert!(rows[1]["data"].get("summary").is_some(), "{}", rows[1]);
    assert!(rows[2]["data"].get("summary").is_some(), "{}", rows[2]);
}

#[test]
fn section_maps_keep_only_the_blocks_with_more_to_read() {
    let mut output = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "sectionPagination": {
                            "files": {
                                "unit": "items",
                                "offset": 0,
                                "length": 1,
                                "total": 1,
                                "hasMore": false
                            },
                            "comments": {
                                "unit": "items",
                                "offset": 0,
                                "length": 0,
                                "total": 4,
                                "hasMore": true
                            },
                            "patches": {
                                "unit": "chars",
                                "offset": 0,
                                "length": 3,
                                "total": 9,
                                "hasMore": true,
                                "nextOffset": 3
                            }
                        }
                    }
                },
                {
                    "index": 1,
                    "data": {
                        "files": [
                            {
                                "path": "a.ts",
                                "patchPagination": {
                                    "unit": "chars",
                                    "offset": 0,
                                    "length": 3,
                                    "total": 3,
                                    "hasMore": false
                                }
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    publish_rows(&mut output);
    assert_eq!(
        output["results"][0]["data"]["sectionPagination"],
        json_fixture(
            r#"{
                "comments": {
                    "unit": "items",
                    "offset": 0,
                    "length": 0,
                    "total": 4,
                    "hasMore": true
                },
                "patches": {
                    "unit": "chars",
                    "offset": 0,
                    "length": 3,
                    "total": 9,
                    "hasMore": true
                }
            }"#
        )
    );
    assert_eq!(
        output["results"][1]["data"],
        json_fixture(r#"{"files": [{"path": "a.ts"}]}"#)
    );
}

/// A section read on one complete page states no block, as any complete
/// first page; a total larger than the page, or a loaded floor, keeps
/// its block.
#[test]
fn a_complete_count_on_its_one_page_states_no_block() {
    let mut output = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "files": ["M +3 -1 a.md"],
                        "sectionPagination": {
                            "files": {
                                "unit": "files",
                                "offset": 0,
                                "length": 1,
                                "total": 1,
                                "countScope": "complete",
                                "hasMore": false
                            }
                        }
                    }
                },
                {
                    "index": 1,
                    "data": {
                        "pagination": {
                            "unit": "items",
                            "offset": 0,
                            "length": 30,
                            "total": 500,
                            "countScope": "complete",
                            "hasMore": false
                        }
                    }
                },
                {
                    "index": 2,
                    "data": {
                        "pagination": {
                            "unit": "items",
                            "offset": 0,
                            "length": 2,
                            "total": 2,
                            "countScope": "loaded",
                            "hasMore": false
                        }
                    }
                }
            ]
        }"#,
    );
    publish_rows(&mut output);
    assert_eq!(
        output["results"][0]["data"],
        json_fixture(r#"{"files": ["M +3 -1 a.md"]}"#)
    );
    assert_eq!(output["results"][1]["data"]["pagination"]["total"], 500);
    assert_eq!(
        output["results"][2]["data"]["pagination"]["countScope"],
        "loaded"
    );
}

#[test]
fn a_page_past_the_end_is_the_rows_out_of_range_status() {
    let mut output = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "status": "empty",
                    "data": {
                        "pagination": {
                            "unit": "items",
                            "offset": 25,
                            "length": 0,
                            "total": 25,
                            "hasMore": false,
                            "outOfRange": true
                        }
                    }
                },
                {
                    "index": 1,
                    "data": {
                        "pagination": {
                            "unit": "items",
                            "offset": 0,
                            "length": 5,
                            "total": 25,
                            "hasMore": true
                        }
                    }
                }
            ]
        }"#,
    );
    state_out_of_range(&mut output);
    assert_eq!(output["results"][0]["status"], "outOfRange");
    assert_eq!(
        output["results"][0]["data"]["pagination"],
        json_fixture(
            r#"{
                "unit": "items",
                "offset": 25,
                "length": 0,
                "total": 25,
                "hasMore": false
            }"#
        )
    );
    assert!(output["results"][1].get("status").is_none());
}

#[test]
fn zero_disclosures_are_absent_and_nonzero_ones_stay() {
    let mut output = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "isPartial": false,
                        "filesSkipped": 0,
                        "diagnostics": [],
                        "warnings": [],
                        "partialReasons": []
                    }
                },
                {
                    "index": 1,
                    "data": {
                        "isPartial": true,
                        "filesSkipped": 2,
                        "diagnostics": [{"message": "m"}]
                    }
                }
            ]
        }"#,
    );
    publish_rows(&mut output);
    assert_eq!(output["results"][0]["data"], json!({}));
    assert_eq!(output["results"][1]["data"]["filesSkipped"], 2);
    assert_eq!(output["results"][1]["data"]["isPartial"], true);
}

#[test]
fn a_structured_value_the_request_stated_is_an_echo_too() {
    let questions = json_fixture(r#"{"a": {"type": "noul"}}"#);
    let queries = [json!({"questions":questions})];
    let mut output = json!({"results":[{"index":0,"data":{
        "questions":questions,"resources":{},
    }}]});
    drop_echoes(false, &mut output, &queries, &[0]);
    assert_eq!(
        output["results"][0]["data"],
        json_fixture(r#"{"resources": {}}"#)
    );
}

#[test]
fn only_a_value_the_request_stated_unchanged_is_an_echo() {
    let queries = [
        json_fixture(r#"{"path": "src", "operation": "files"}"#),
        json_fixture(r#"{"path": "/abs/src"}"#),
    ];
    let mut output = json_fixture(
        r#"{
            "results": [
                {"index": 1, "data": {"path": "src", "operation": "tree"}},
                {"index": 0, "data": {"error": "bad input", "path": "src"}},
                {
                    "index": 2,
                    "data": {"path": "/abs/src", "operation": "files", "files": []}
                }
            ]
        }"#,
    );
    // Query positions 0 and 1 answer original rows 0 and 2; row 1 was rejected.
    drop_echoes(false, &mut output, &queries, &[0, 2]);
    assert_eq!(
        output["results"],
        json_fixture(
            r#"[
                {"index": 1, "data": {"path": "src", "operation": "tree"}},
                {"index": 0, "data": {"error": "bad input"}},
                {"index": 2, "data": {"operation": "files", "files": []}}
            ]"#
        )
    );
}
#[test]
fn a_commit_pinned_ref_drops_only_a_file_reads_commit_sha() {
    let sha = "8a6a9492d0bc897ebb455fc1ab6916fb34e7b55a";
    let queries = [json!({"owner":"o","repo":"r","ref":sha,"path":"a.rs"})];
    let mut read = json!({"results":[{"index":0,"data":{"commitSha":sha,"content":"x"}}]});
    drop_echoes(true, &mut read, &queries, &[0]);
    assert_eq!(
        read["results"][0]["data"],
        json_fixture(r#"{"content": "x"}"#)
    );

    let queries = [json!({"owner":"o","repo":"r","operation":"commit","ref":sha})];
    let mut commit = json!({"results":[{"index":0,"data":{"commitSha":sha,"message":"m"}}]});
    drop_echoes(false, &mut commit, &queries, &[0]);
    assert_eq!(commit["results"][0]["data"]["commitSha"], sha);
}
