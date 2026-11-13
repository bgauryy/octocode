use super::validate_wire;
use crate::providers::classification::ClassificationProvider;
use serde_json::{Value, json};

fn request() -> Value {
    json!({"model":"jev-test","state":null,"questions":{
        "truth":{"type":"noul","instructions":null},
        "pick":{"type":"choice","instructions":null,"criteria":{"a":null,"b":{"meaning":"other"}}},
        "quality":{"type":"score","instructions":null,"criteria":["missing",{"meaning":"good"}]},
        "region":{"type":"box"}
    }})
}

fn response() -> Value {
    json!({"model":"jev-test","usage":{"input_tokens":12,"output_tokens":3},"answers":{
        "truth":{"type":"noul","noul":0.5},
        "pick":{"type":"choice","choice":"a","confidence":0.8,"probabilities":{"a":0.8,"b":0.2}},
        "quality":{"type":"score","score":0.75,"confidence":0.7,"probabilities":{"0":0.25,"1":0.75},"legend":{"0":"missing","1":{"meaning":"good"}}},
        "region":{"type":"box","box":{"top":0.1,"bottom":0.9,"left":0.0,"right":1.0},"confidence":null}
    }})
}

#[test]
fn validates_all_primitives_without_interpreting_ambiguous_judgments() {
    validate_wire(&request(), &response()).unwrap();
}

#[test]
fn score_agrees_with_distribution_allowing_two_decimal_rounding() {
    for (levels, probabilities, score, valid) in [
        (3, vec![0.21, 0.66, 0.13], 0.92, true),
        (3, vec![0.21, 0.66, 0.13], 0.93, true),
        (3, vec![0.21, 0.66, 0.13], 0.96, false),
        (3, vec![1.0, 0.0, 0.0], 2.0, false),
        (2, vec![0.25, 0.75], 0.76, true),
        (2, vec![0.25, 0.75], 0.77, false),
        (10, vec![0.10; 10], 4.725, true),
        (10, vec![0.10; 10], 4.74, false),
        (100, vec![0.01; 100], 49.5, true),
        (100, vec![0.01; 100], 49.74, true),
        (100, vec![0.01; 100], 49.76, false),
        (100, vec![0.01; 100], 74.0, false),
    ] {
        let criteria: Vec<_> = (0..levels).map(|i| json!(format!("Level {i}"))).collect();
        let legend: serde_json::Map<String, Value> = criteria
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v.clone()))
            .collect();
        let probabilities: serde_json::Map<String, Value> = probabilities
            .iter()
            .enumerate()
            .map(|(i, p)| (i.to_string(), json!(p)))
            .collect();
        let request = json!({"questions":{"q":{"type":"score","criteria":criteria}}});
        let response = json!({"model":"m","usage":{"input_tokens":1,"output_tokens":1},
                "answers":{"q":{"type":"score","score":score,"legend":legend,
                    "confidence":0.4,"probabilities":probabilities}}});
        assert_eq!(
            validate_wire(&request, &response).is_ok(),
            valid,
            "levels={levels}, score={score}"
        );
    }
}

#[test]
fn rejects_incompatible_answers_and_malformed_metadata() {
    for (pointer, value) in [
        ("/model", json!("")),
        ("/usage/input_tokens", json!(-1)),
        ("/answers/truth/type", json!("score")),
        ("/answers/truth/noul", json!(1.1)),
        ("/answers/pick/choice", json!("b")),
        ("/answers/pick/confidence", json!(1.1)),
        ("/answers/pick/probabilities", json!({"a":0.8,"c":0.2})),
        ("/answers/pick/probabilities", json!({"a":0.8,"b":0.4})),
        ("/answers/quality/score", json!(2)),
        ("/answers/quality/legend", json!({"0":null,"1":"different"})),
    ] {
        let mut invalid = response();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert!(
            validate_wire(&request(), &invalid).is_err(),
            "accepted {pointer}"
        );
    }
    let mut missing = response();
    missing["answers"].as_object_mut().unwrap().remove("truth");
    assert!(validate_wire(&request(), &missing).is_err());
    let mut extra = response();
    extra["answers"]["extra"] = json!({"type":"noul","noul":0.9});
    assert!(validate_wire(&request(), &extra).is_err());
    let mut missing_usage = response();
    missing_usage["usage"] = json!({});
    assert!(validate_wire(&request(), &missing_usage).is_err());
    assert!(validate_wire(&json!({}), &response()).is_err());
}

#[test]
fn box_requires_finite_normalized_ordered_edges_and_present_nullable_confidence() {
    let request = json!({"questions":{"region":{"type":"box"}}});
    let valid = json!({"model":"vision","usage":{"input_tokens":10,"output_tokens":4},
            "answers":{"region":{"type":"box","box":{"top":0.1,"bottom":0.9,"left":0.2,"right":0.7},"confidence":null}}});
    validate_wire(&request, &valid).unwrap();
    let mut numeric_confidence = valid.clone();
    numeric_confidence["answers"]["region"]["confidence"] = json!(0.7);
    validate_wire(&request, &numeric_confidence).unwrap();
    for (pointer, value) in [
        ("/answers/region/box/top", json!(-0.1)),
        ("/answers/region/box/bottom", json!(1.1)),
        ("/answers/region/box/top", json!(0.95)),
        ("/answers/region/box/top", json!(0.9)),
        ("/answers/region/box/right", json!(0.2)),
        ("/answers/region/box/right", json!(0.1)),
        ("/answers/region/box/left", Value::Null),
        ("/answers/region/box/left", json!("NaN")),
        ("/answers/region/confidence", json!(1.1)),
    ] {
        let mut invalid = valid.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert!(
            validate_wire(&request, &invalid).is_err(),
            "accepted {pointer}"
        );
    }
    let mut missing = valid;
    missing["answers"]["region"]
        .as_object_mut()
        .unwrap()
        .remove("confidence");
    assert!(validate_wire(&request, &missing).is_err());
}

#[test]
fn rejects_undeclared_fields_in_each_answer_shape() {
    for pointer in [
        "/answers/truth",
        "/answers/pick",
        "/answers/quality",
        "/answers/region",
        "/answers/region/box",
    ] {
        let mut invalid = response();
        invalid
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!("extension"));
        assert!(
            validate_wire(&request(), &invalid).is_err(),
            "accepted {pointer}"
        );
    }
}

#[test]
fn request_builder_preserves_structured_optional_and_null_question_fields() {
    let questions = [
        json!({"type":"noul"}),
        json!({"type":"noul","instructions":null,"criteria":{"true":null}}),
        json!({"type":"choice","criteria":{"only":null}}),
        json!({"type":"score","instructions":{"task":["grade"]},"criteria":[{"level":"present"}]}),
        json!({"type":"box","instructions":["locate","opaque"]}),
    ];
    let assets = json!({"opaque":{"type":"image","format":"png","data_base64":"bytes"}});
    let request = super::JEV.build(
        &json!(["opaque"]),
        &questions.iter().collect::<Vec<_>>(),
        "vision",
        Some(&assets),
    );
    assert_eq!(request["assets"], assets);
    for (index, question) in questions.iter().enumerate() {
        assert_eq!(&request["questions"][format!("answer_{index}")], question);
    }
    let plain = super::JEV.build(&json!("text"), &[&questions[0]], "m", None);
    assert_eq!(plain["questions"], json!({"answer": questions[0]}));
    assert!(plain.get("assets").is_none());
    assert!(super::JEV.headers(false, false).is_empty());
    let headers = super::JEV.headers(true, true);
    assert_eq!(headers["TypeSafe-Danger-Zone-Images"], "true");
    assert_eq!(headers["TypeSafe-Danger-Zone-Long-Context"], "true");
}

/// One question is one whole response; the model and usage are read
/// even from a response that fails validation.
#[test]
fn a_single_question_response_reads_its_answer_model_and_usage() {
    let question = json!({"type":"noul","instructions":"q"});
    let request = super::JEV.build(&json!("s"), &[&question], "m", None);
    let response = json!({"model":"jev-1","usage":{"input_tokens":7,"output_tokens":2},
            "answers":{"answer":{"type":"noul","noul":0.4}}});
    let read = super::JEV.read(&request, &response);
    assert_eq!(
        read.answers.expect("valid response"),
        vec![Ok(json!({"type":"noul","noul":0.4}))]
    );
    assert_eq!(read.model.as_deref(), Some("jev-1"));
    assert_eq!(
        read.usage,
        Some(super::ProviderUsage {
            input_tokens: 7,
            output_tokens: 2
        })
    );
    let mut invalid = response;
    invalid["answers"]["answer"]["noul"] = json!(2);
    let read = super::JEV.read(&request, &invalid);
    assert!(read.answers.is_err());
    assert_eq!(read.usage.map(|usage| usage.input_tokens), Some(7));
    invalid["usage"] = json!({"input_tokens": 1});
    assert_eq!(super::JEV.read(&request, &invalid).usage, None);
}

/// One listed model as the output contract types it (`ClasifyModelMetadata`).
fn listed(name: &str, description: &str, release_date: &str) -> Value {
    serde_json::to_value(octocode_contract_types::ClasifyModelMetadata {
        default: None,
        description: description.to_owned(),
        name: name.to_owned(),
        release_date: release_date.to_owned(),
    })
    .expect("model metadata")
}

/// A listing value that cannot be serialized fails the listing with a
/// typed response error; it is never listed as `null`.
#[test]
fn a_listing_value_that_cannot_serialize_is_a_response_error_not_null() {
    struct Unserializable;
    impl serde::Serialize for Unserializable {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("refused"))
        }
    }
    let error = super::wire_value(&Unserializable).expect_err("a serialize error is surfaced");
    assert!(!error.request, "{error:?}");
    assert!(error.message.contains("refused"), "{error:?}");
}

/// The listing leaves the vendor wire spelled as the output contract's
/// generated type spells it, never as the wire does, so a contract rename
/// reaches every listed model (ALL-TOOLS-C D12).
#[test]
fn model_listing_spells_its_fields_as_the_output_contract_types_them() {
    let wire = json!({"models":[{"name":"m","description":"d","release_date":"2026-01-01"}]});
    assert_eq!(
        super::JEV.models(&wire).unwrap(),
        json!({"models":[listed("m", "d", "2026-01-01")]})
    );
    let local = json!({"data":[{"id":"m"}]});
    assert_eq!(
        super::JEV.models(&local).unwrap(),
        json!({"models":[listed("m", "", "")]})
    );
}

#[test]
fn model_discovery_accepts_live_timestamp_strings_and_rejects_missing_fields() {
    let response = json!({"object":"list","models":[{"name":"jev-preview","description":"Preview",
            "release_date":"2026-09-10T18:39:06.057655+00:00","owner":"typesafe"}]});
    // Only the clasify output fields leave the vendor wire.
    assert_eq!(
        super::JEV.models(&response).unwrap(),
        json!({"models":[listed("jev-preview", "Preview", "2026-09-10T18:39:06.057655+00:00")]})
    );
    assert_eq!(
        super::JEV.models(&json!({"models":[]})).unwrap(),
        json!({"models":[]})
    );
    for invalid in [
        json!({}),
        json!({"models":null}),
        json!({"models":[{"name":""}]}),
        json!({"models":[{"name":"m","description":"d","release_date":null}]}),
        json!({"data":[{"id":""}]}),
    ] {
        assert!(super::JEV.models(&invalid).is_err());
    }
}

#[test]
fn model_discovery_reads_a_local_servers_openai_list_without_inventing_fields() {
    // Ollama's `/v1/models`, served through FreeLlama.
    let response = json!({"object":"list","data":[
        {"id":"strands-decider:2b-nvfp4","object":"model","created":1793359804,"owned_by":"library"}
    ]});
    assert_eq!(
        super::JEV.models(&response).unwrap(),
        json!({"models":[listed("strands-decider:2b-nvfp4", "", "")]})
    );
}

/// A question is the caller's words as the input gate admitted them: what the
/// gate cleared (a key's armor line, a placeholder URL, a declaration) is
/// sent as written, and text only a file's rule hides is still hidden. The
/// state is evidence and is judged as kept text is.
#[test]
fn an_admitted_question_is_sent_as_written_and_the_state_is_judged() {
    const VALUE: &str = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz";
    let manifest = format!("apiVersion: v1\nkind: Secret\ndata:\n  tls.crt: {VALUE}\n");
    let token = format!("ghp_{}", "a".repeat(36));
    for words in [
        "Does it contain -----BEGIN RSA PRIVATE KEY-----?",
        "Does it connect to postgres://user:password@localhost:5432/db",
        "Is password: str declared, or token: Token, or api_key: str = None?",
        "Is dispatchToken: string a field?",
    ] {
        let question = json!({"type":"noul","instructions": words});
        let request = super::JEV.build(&json!("text"), &[&question], "m", None);
        assert_eq!(request["questions"]["answer"], question, "{words}");
    }
    let question = json!({"type":"noul","instructions": format!("Is this applied?\n{manifest}")});
    let state = json!({"text": format!("-----BEGIN RSA PRIVATE KEY-----\nconst t = \"{token}\";")});
    let request = super::JEV
        .build(&state, &[&question], "m", None)
        .to_string();
    assert!(
        !request.contains(VALUE) && !request.contains(&token),
        "{request}"
    );
    // The armor holds no key and reads as written; the token beside it goes.
    assert!(request.contains("BEGIN RSA PRIVATE KEY"), "{request}");
}
