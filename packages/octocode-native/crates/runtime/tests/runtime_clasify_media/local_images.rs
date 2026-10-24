//! Local images: byte-exact hiding, mutation invalidation, placeholders and multiple assets, and refused images.

use super::{png, provider, requests, row, runtime};
use crate::runtime_clasify::clasify;
use crate::support::Workspace;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

#[tokio::test]
async fn unread_local_image_is_byte_exact_hidden_and_saved_with_box() {
    let server = provider().await;
    let workspace = Workspace::new();
    let bytes = png();
    std::fs::write(workspace.workspace.join("one.png"), &bytes).unwrap();
    let runtime = runtime(&workspace, &server, &[]);
    let out = clasify(&runtime, vec![row("one.png")]).await;
    let result = &out["results"][0]["data"];
    assert_eq!(result["answers"]["bounds"]["box"]["left"], 0.2, "{out}");
    let asset = &result["assets"]["image_a"];
    assert_eq!(
        (asset["width"].as_u64(), asset["height"].as_u64()),
        (Some(280), Some(280))
    );
    assert_eq!(asset["format"], "png");
    assert_eq!(
        std::fs::read(asset["path"].as_str().unwrap()).unwrap(),
        bytes
    );
    assert!(asset["snapshot"].as_str().unwrap().len() == 64);
    assert!(
        result.get("hints").is_none(),
        "images do not advertise localFetch of binary"
    );
    assert!(!out.to_string().contains("data_base64"));
    assert!(!out.to_string().contains(&STANDARD.encode(&bytes)));
    let sent = requests(&server).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].headers.get("typesafe-danger-zone-images").unwrap(),
        "true"
    );
    let body: Value = serde_json::from_slice(&sent[0].body).unwrap();
    assert_eq!(
        body["assets"]["image_a"]["data_base64"],
        STANDARD.encode(&bytes)
    );
    assert_eq!(body["state"], "image_a");
    runtime.close().await;
}

#[tokio::test]
async fn image_mutation_invalidates_judgment_and_old_evidence_survives() {
    let server = provider().await;
    let workspace = Workspace::new();
    let path = workspace.workspace.join("one.png");
    std::fs::write(&path, png()).unwrap();
    let runtime = runtime(&workspace, &server, &[]);
    let first = clasify(&runtime, vec![row("one.png")]).await;
    let repeated = clasify(&runtime, vec![row("one.png")]).await;
    assert_eq!(repeated["results"][0]["data"]["replayed"], true);
    let old = &first["results"][0]["data"]["assets"]["image_a"];
    let mut changed = png();
    changed.push(0);
    std::fs::write(&path, &changed).unwrap();
    let newer = clasify(&runtime, vec![row("one.png")]).await;
    assert_ne!(
        newer["results"][0]["data"]["assets"]["image_a"]["snapshot"],
        old["snapshot"]
    );
    assert_eq!(std::fs::read(old["path"].as_str().unwrap()).unwrap(), png());
    assert_eq!(requests(&server).await.len(), 2);
    runtime.close().await;
}

#[tokio::test]
async fn question_only_placeholders_and_multiple_assets_are_preserved() {
    let server = provider().await;
    let workspace = Workspace::new();
    std::fs::write(workspace.workspace.join("one.png"), png()).unwrap();
    let runtime = runtime(&workspace, &server, &[]);
    let mut query = row("one.png");
    query["content"] = json!({"purpose":"screen two independent images"});
    query["assets"]["image_b"] = query["assets"]["image_a"].clone();
    query["questions"]["visible"]["instructions"] =
        json!("Does image_a show the same shape as image_b?");
    let out = clasify(&runtime, vec![query]).await;
    assert_eq!(
        out["results"][0]["data"]["assets"]
            .as_object()
            .unwrap()
            .len(),
        2,
        "{out}"
    );
    let sent = requests(&server).await;
    let body: Value = serde_json::from_slice(&sent[0].body).unwrap();
    assert_eq!(body["assets"].as_object().unwrap().len(), 2);
    runtime.close().await;
}

#[tokio::test]
async fn invalid_missing_oversized_and_private_images_never_reach_provider() {
    let server = provider().await;
    let workspace = Workspace::new();
    std::fs::write(workspace.workspace.join("bad.png"), b"not an image").unwrap();
    let mut large_dimensions = png();
    large_dimensions[16..20].copy_from_slice(&9000u32.to_be_bytes());
    std::fs::write(workspace.workspace.join("wide.png"), large_dimensions).unwrap();
    let oversized = std::fs::File::create(workspace.workspace.join("large.png")).unwrap();
    oversized.set_len(8 * 1024 * 1024 + 1).unwrap();
    let runtime = runtime(&workspace, &server, &[]);
    let mut unused = row("missing.png");
    unused["assets"]["unused"] = unused["assets"]["image_a"].take();
    unused["assets"].as_object_mut().unwrap().remove("image_a");
    let mut url = row("unused");
    url["assets"]["image_a"]["source"] = json!({"url":"https://127.0.0.1:9/image.png"});
    let out = clasify(
        &runtime,
        vec![
            row("bad.png"),
            row("wide.png"),
            row("large.png"),
            unused,
            url,
        ],
    )
    .await;
    let errors: Vec<_> = out["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["data"]["errorCode"].as_str().unwrap())
        .collect();
    assert_eq!(
        errors,
        [
            "invalidClassificationContext",
            "classificationContextTooLarge",
            "classificationContextTooLarge",
            "invalidClassificationContext",
            "securityValidationFailed"
        ],
        "{out}"
    );
    assert!(requests(&server).await.is_empty());
    runtime.close().await;
}
