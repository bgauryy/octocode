use super::*;

fn context(configuration: Value, section_root: Option<&str>) -> ClientRequestContext {
    ClientRequestContext {
        configuration,
        section_root: section_root.map(str::to_owned),
        workspace_folders: json!([{ "uri": "file:///w", "name": "workspace" }]),
    }
}

fn configuration(context: &ClientRequestContext, items: Value) -> Value {
    match client_response_for(
        "workspace/configuration",
        Some(&json!({ "items": items })),
        context,
    ) {
        ClientResponse::Result(value) => value,
        other => panic!("configuration must be a result, got {other:?}"),
    }
}

#[test]
fn configuration_answers_one_entry_per_item_by_section() {
    let wrapped = context(json!({"gopls": {"a": 1}, "other": true}), None);
    assert_eq!(
        configuration(
            &wrapped,
            json!([{"section": "gopls"}, {"section": "missing"}, {}, {"scopeUri": "file:///w"}])
        ),
        json!([{"a": 1}, null, {"gopls": {"a": 1}, "other": true}, {"gopls": {"a": 1}, "other": true}])
    );
}

#[test]
fn configuration_resolves_unwrapped_server_section() {
    let unwrapped = context(
        json!({"checkOnSave": false, "cargo": {"features": []}}),
        Some("rust-analyzer"),
    );
    assert_eq!(
        configuration(
            &unwrapped,
            json!([
                {"section": "rust-analyzer"},
                {"section": "rust-analyzer.cargo"},
                {"section": "rust-analyzer.missing"},
                {"section": "gopls"}
            ])
        ),
        json!([
            {"checkOnSave": false, "cargo": {"features": []}},
            {"features": []},
            null,
            null
        ])
    );
    // Without a section root the same unwrapped object is NOT the section.
    let unknown = context(json!({"checkOnSave": false}), None);
    assert_eq!(
        configuration(&unknown, json!([{"section": "rust-analyzer"}])),
        json!([null])
    );
    // A wrapped object under a declared root still resolves the wrapper key.
    let wrapped = context(json!({"gopls": {"a": 1}}), Some("gopls"));
    assert_eq!(
        configuration(&wrapped, json!([{"section": "gopls"}])),
        json!([{"a": 1}])
    );
}

#[test]
fn configuration_walks_dotted_sections_and_flat_keys() {
    let settings = context(
        json!({
            "java": {"format": {"tabSize": 4}},
            "python.analysis": {"diagnosticMode": "openFilesOnly"}
        }),
        None,
    );
    assert_eq!(
        configuration(
            &settings,
            json!([
                {"section": "java.format.tabSize"},
                {"section": "python.analysis"},
                {"section": "java.format.missing"},
                {"section": "java.format.tabSize.deeper"}
            ])
        ),
        json!([4, {"diagnosticMode": "openFilesOnly"}, null, null])
    );
    // No items (or a malformed params object) → an empty array.
    assert_eq!(configuration(&settings, json!([])), json!([]));
}

#[test]
fn client_response_for_unknown_method_is_method_not_found() {
    let context = context(json!({ "settings": true }), None);
    match client_response_for("nonexistent/method", None, &context) {
        ClientResponse::Error { code, .. } => assert_eq!(code.as_i64(), -32601),
        other => panic!("expected -32601 MethodNotFound, got {other:?}"),
    }
    match client_response_for("workspace/workspaceFolders", None, &context) {
        ClientResponse::Result(value) => assert_eq!(value, context.workspace_folders),
        other => panic!("expected workspaceFolders result, got {other:?}"),
    }
    match client_response_for("workspace/applyEdit", None, &context) {
        ClientResponse::Result(value) => assert_eq!(value, json!({ "applied": false })),
        other => panic!("expected applyEdit result, got {other:?}"),
    }
    for method in [
        "client/registerCapability",
        "window/workDoneProgress/create",
    ] {
        match client_response_for(method, None, &context) {
            ClientResponse::Result(Value::Null) => {}
            other => panic!("{method} must stay a null result, got {other:?}"),
        }
    }
}

#[test]
fn replies_echo_the_request_id_unchanged() {
    let reply = ClientResponse::Error {
        code: ErrorCode::MethodNotFound,
        message: "x".to_owned(),
    }
    .into_message(json!("abc"));
    assert_eq!(reply["id"], "abc");
    assert_eq!(reply["error"]["code"], -32601);
}
