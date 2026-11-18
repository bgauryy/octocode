use super::*;
use octocode_test_support::json_fixture;

fn source_row() -> Row {
    Row {
        context: None,
        model: None,
        keys: vec![],
        questions: vec![],
        input: Input::Judge(Target::Source(json_fixture(
            r#"{
                "tool": "localFetch",
                "query": {
                    "path": "source.rs",
                    "ranges": [{"startLine": 8, "endLine": 12}]
                }
            }"#,
        ))),
        assets: None,
        long_context: None,
        details: true,
        per_item: false,
        source_progress: None,
    }
}

#[test]
fn failed_judgment_retains_exact_captured_read_and_provider_guidance_without_source_body() {
    let row = source_row();
    let Input::Judge(Target::Source(source)) = &row.input else {
        unreachable!()
    };
    let snapshot = "f".repeat(64);
    let capture = Captured {
        read_chars: 0,
        read_pages: false,
        state: json_fixture(r#"{"content": "PRIVATE SOURCE BODY"}"#),
        read: source_capture::read_lead(
            ToolId::LocalFetch,
            &source["query"],
            Some(snapshot.as_str()),
        ),
        source: None,
        pin: None,
    };
    let mut failure = json_fixture(
        r#"{
            "errorCode": "classificationProviderError",
            "hints": ["Provider guidance."]
        }"#,
    );
    add(&row, Some(&capture), &mut failure, true);
    assert_eq!(failure["next"]["read"], capture.read);
    assert_eq!(failure["next"]["read"]["kind"], "continue");
    assert_eq!(
        failure["next"]["read"]["input"]["queries"][0]["snapshot"],
        json!(snapshot)
    );
    assert!(
        failure["hints"]
            .as_array()
            .unwrap()
            .contains(&json!("Provider guidance."))
    );
    assert!(!failure.to_string().contains("PRIVATE SOURCE BODY"));
    let once = failure.clone();
    add(&row, Some(&capture), &mut failure, true);
    assert_eq!(failure, once);
}

#[test]
fn acquisition_failure_offers_fresh_scoped_retry_only_when_repeat_can_succeed() {
    let row = source_row();
    let Input::Judge(Target::Source(source)) = &row.input else {
        unreachable!()
    };
    let mut failure = json_fixture(r#"{"errorCode": "classificationContextFailed"}"#);
    add(&row, None, &mut failure, true);
    assert_eq!(failure["related"][0]["kind"], "related");
    assert_eq!(
        failure["related"][0]["input"]["queries"][0],
        source["query"]
    );
    assert!(failure.get("next").is_none());
    // Too large to judge whole: no read of it, a ready call in parts.
    let mut asked = source_row();
    asked.keys = vec!["q".into()];
    asked.questions = vec![json_fixture(
        r#"{"type": "noul", "instructions": "Is it big?"}"#,
    )];
    let mut too_large = json_fixture(r#"{"errorCode": "classificationContextTooLarge"}"#);
    add(&asked, None, &mut too_large, true);
    assert!(too_large.get("next").is_none(), "{too_large}");
    let split = &too_large["related"][0]["input"]["queries"][0];
    assert_eq!(split["operation"], "sections", "{too_large}");
    assert_eq!(split["resources"]["source"], *source, "{too_large}");
    // A runtime that keeps nothing for a later call refuses a plan: it
    // offers none.
    let mut one_shot = json_fixture(r#"{"errorCode": "classificationContextTooLarge"}"#);
    add(&asked, None, &mut one_shot, false);
    assert!(one_shot.get("related").is_none(), "{one_shot}");
    for code in ["notFound", "outsideAllowedRoots"] {
        let mut failure = json!({"errorCode":code});
        add(&row, None, &mut failure, true);
        assert!(failure.get("next").is_none(), "{failure}");
        assert!(failure.get("related").is_none(), "{failure}");
    }
}

/// A provider window an image row exceeds is cut by asking fewer image
/// questions per call; a text row keeps only the provider's own advice.
#[test]
fn an_image_row_over_the_provider_window_says_to_ask_fewer_image_questions() {
    let mut row = source_row();
    row.input = Input::Judge(Target::Content(json!("literal")));
    let failure = || {
        json_fixture(
            r#"{
                "errorCode": "classificationStateTooLarge",
                "hints": ["Provider guidance."]
            }"#,
        )
    };
    let mut text = failure();
    add(&row, None, &mut text, true);
    assert!(!text.to_string().contains(IMAGE_WINDOW_HINT), "{text}");
    row.assets = Some(json_fixture(r#"{"image": {}}"#));
    let mut image = failure();
    add(&row, None, &mut image, true);
    let hints = image["hints"].as_array().unwrap();
    assert!(hints.contains(&json!(IMAGE_WINDOW_HINT)), "{image}");
    assert!(hints.contains(&json!("Provider guidance.")), "{image}");
    let once = image.clone();
    add(&row, None, &mut image, true);
    assert_eq!(image, once);
    let mut other = json_fixture(r#"{"errorCode": "classificationProviderError"}"#);
    add(&row, None, &mut other, true);
    assert!(!other.to_string().contains(IMAGE_WINDOW_HINT), "{other}");
}

/// An input refusal judged nothing: it keeps only its own input-fix
/// guidance, never the notices for a failed judgment or for inspecting
/// the images some other way.
#[test]
fn an_input_refusal_carries_only_its_input_fix() {
    let mut row = source_row();
    row.input = Input::Judge(Target::Content(json!("literal")));
    row.assets = Some(json_fixture(r#"{"pic": {}}"#));
    let notice = octocode_runtime_base::contracts::classification_notice;
    for code in [
        "invalidClassificationContext",
        "invalidClassificationRequest",
    ] {
        let mut refused = json!({"errorCode":code,"hints":["Fix the input."]});
        add(&row, None, &mut refused, true);
        assert_eq!(
            refused["hints"],
            json_fixture(r#"["Fix the input."]"#),
            "{refused}"
        );
    }
    // A judgment that failed still says so, with the image route.
    let mut failed = json_fixture(r#"{"errorCode": "classificationProviderError"}"#);
    add(&row, None, &mut failed, true);
    let hints = failed["hints"].as_array().unwrap();
    assert!(hints.contains(&json!(notice("failed"))), "{failed}");
    assert!(
        hints.contains(&json!(notice("imageInspection"))),
        "{failed}"
    );
}

/// An image given as a text source gets the same questions as a ready
/// call with it under assets.
#[test]
fn a_binary_source_offers_the_same_questions_with_the_file_under_assets() {
    let mut row = source_row();
    row.keys = vec!["q".into()];
    row.questions = vec![json_fixture(
        r#"{"type": "noul", "instructions": "Is a popup shown?"}"#,
    )];
    let mut failure = json_fixture(
        r#"{
            "errorCode": "binaryFileUnsupported",
            "hints": ["Reader advice."]
        }"#,
    );
    add(&row, None, &mut failure, true);
    let hints = failure["hints"].as_array().unwrap();
    assert_eq!(hints[0], json!(IMAGE_SOURCE_HINT), "{failure}");
    assert!(hints.contains(&json!("Reader advice.")), "{failure}");
    let call = failure["related"]
        .as_array()
        .unwrap()
        .iter()
        .find(|lead| lead["tool"] == "clasify")
        .expect("assets call");
    let query = &call["input"]["queries"][0];
    assert_eq!(
        query["assets"]["image"]["source"],
        json_fixture(r#"{"tool": "localFetch", "query": {"path": "source.rs"}}"#),
        "{failure}"
    );
    assert_eq!(query["content"], "image");
    assert_eq!(query["questions"]["q"], row.questions[0]);
}

/// A glob in `source` names no file; the row says globs go
/// in `resources` and offers the same row with the glob there.
#[test]
fn a_glob_in_source_points_to_resources() {
    let mut row = source_row();
    row.keys = vec!["q".into()];
    row.questions = vec![json_fixture(
        r#"{"type": "noul", "instructions": "Is it set?"}"#,
    )];
    let glob = json_fixture(r#"{"tool": "localFetch", "query": {"path": "w/*.md"}}"#);
    row.input = Input::Judge(Target::Source(glob.clone()));
    let tree = json_fixture(
        r#"{
            "tool": "structureSearch",
            "kind": "related",
            "input": {"queries": [{"operation": "tree", "path": "w"}]}
        }"#,
    );
    let mut failure = json!({"errorCode":"notFound","error":"Path does not exist: w/*.md",
        "related":[tree]});
    add(&row, None, &mut failure, true);
    let hints = failure["hints"].as_array().cloned().unwrap_or_default();
    assert!(
        hints
            .iter()
            .any(|hint| hint.as_str().is_some_and(|hint| hint.contains("resources"))),
        "{failure:#}"
    );
    // The way to the files leads; the directory listing stays after it.
    let call = &failure["related"][0];
    assert_eq!(call["tool"], "clasify", "first: {failure:#}");
    assert_eq!(failure["related"][1], tree, "{failure:#}");
    let query = &call["input"]["queries"][0];
    assert_eq!(query["operation"], "evaluate", "{call:#}");
    assert_eq!(query["resources"], json!({"source":glob}), "{call:#}");
    // A literal missing path is no glob: nothing points to resources.
    let mut plain = json_fixture(
        r#"{
            "errorCode": "notFound",
            "error": "Path does not exist: source.rs"
        }"#,
    );
    add(&source_row(), None, &mut plain, true);
    assert!(!plain.to_string().contains("resources"), "{plain:#}");
}

#[test]
fn literal_content_and_model_discovery_do_not_fabricate_source_reads() {
    let mut row = source_row();
    row.input = Input::Judge(Target::Content(json!("literal")));
    let mut failure = json_fixture(r#"{"errorCode": "classificationProviderError"}"#);
    add(&row, None, &mut failure, true);
    assert!(failure.get("next").is_none());
    assert!(failure.get("related").is_none());
    row.input = Input::Models;
    let before = failure.clone();
    add(&row, None, &mut failure, true);
    assert_eq!(failure, before);
}
