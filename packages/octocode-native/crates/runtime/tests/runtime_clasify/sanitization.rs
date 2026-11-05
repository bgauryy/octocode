use super::*;

/// A JSON object's structure is evidence, just as its serialized form is.
/// Keep the public members, folders, types and question ids while hiding data.
#[tokio::test]
async fn sensitive_boundary_structured_content_and_questions_keep_their_context() {
    let secret = "SyntheticCredentialMaterial987654321abcdef";
    let folder = "src/secrets/credentials/private/source.pem";
    let documents = [
        json!({"password":secret,"folder":folder,"enabled":true,"count":3}),
        json!({"kind":"Secret","data":{"payload":secret},"folder":folder}),
        json!({"nested":[{"d":secret,"kty":"RSA","n":"publicModulus","e":"AQAB"}],"folder":folder}),
    ];
    for document in documents {
        let workspace = crate::support::Workspace::new();
        let server = provider(1).await;
        let runtime = crate::support::clasify_runtime(&workspace, &server, &[]);
        let result = clasify(
            &runtime,
            vec![json!({
                "operation":"evaluate", "content":document,
                "questions":{"inspect":{"type":"noul","instructions":document}}
            })],
        )
        .await;
        assert!(
            result["results"][0]["data"]["answers"]["inspect"].is_object(),
            "{result}"
        );
        let requests = sent(&server).await;
        let request = &requests[0];
        assert!(!request.to_string().contains(secret), "{request}");
        assert!(request.to_string().contains("REDACTED"), "{request}");
        assert_eq!(request["state"]["folder"], folder);
        assert_eq!(
            request["questions"]["answer"]["instructions"]["folder"],
            folder
        );
        if document.get("enabled").is_some() {
            assert_eq!(request["state"]["enabled"], true);
            assert_eq!(request["state"]["count"], 3);
        }
    }
}
