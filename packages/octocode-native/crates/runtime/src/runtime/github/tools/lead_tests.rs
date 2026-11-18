use super::drop_withheld_reads;
use octocode_runtime_base::policy::path::{PathPolicy, PathPolicyConfig};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

fn policy(allow: bool) -> PathPolicy {
    let root = tempfile::tempdir().expect("root");
    PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.path().to_path_buf()),
        allow_credential_files: allow,
        ..Default::default()
    })
    .expect("policy")
}

fn read(paths: &[&str]) -> Value {
    let rows: Vec<Value> = paths
        .iter()
        .map(|path| json!({"owner":"o","repo":"r","path":path}))
        .collect();
    json!({"kind":"related","tool":"ghGetFileContent","input":{"queries":rows}})
}

fn row() -> Value {
    json!({
        "files": [{"path": ".env", "related": [read(&[".env"]), read(&["src/a.rs"])]}],
        "next": {"read": read(&[".env", ".ssh/config", "src/a.rs"])},
        "calls": [read(&[".ssh/config"]),
            {"kind":"related","tool":"ghSearchCode","input":{"queries":[{"owner":"o","path":".env"}]}}]
    })
}

#[test]
fn leads_to_withheld_files_are_dropped_with_one_warning() {
    let mut data = row();
    drop_withheld_reads(&mut data, &policy(false));
    let text = data.to_string();
    assert!(!text.contains("\"path\":\".ssh/config\""), "{text}");
    assert_eq!(
        data["files"][0]["related"].as_array().map(Vec::len),
        Some(1),
        "{text}"
    );
    assert_eq!(
        data["next"]["read"]["input"]["queries"],
        json_fixture(r#"[{"owner": "o", "repo": "r", "path": "src/a.rs"}]"#)
    );
    // A call of another tool is not a read of the file.
    assert_eq!(data["calls"].as_array().map(Vec::len), Some(1), "{text}");
    assert_eq!(data["warnings"].as_array().map(Vec::len), Some(1), "{text}");
    // Four query rows go, from three calls: the warning counts rows.
    assert!(
        text.contains("4 ghGetFileContent read row(s) were dropped from leads"),
        "{text}"
    );
}

#[test]
fn a_container_the_pass_empties_goes_with_its_last_lead() {
    let mut data = json!({
        "files": [{"path": ".env", "related": [read(&[".env"])]}],
        "next": {"read": read(&[".env"])},
        "calls": [read(&[".ssh/config"])],
        "kept": {"empty": {}, "none": []}
    });
    drop_withheld_reads(&mut data, &policy(false));
    let text = data.to_string();
    assert!(data.get("next").is_none(), "no next:{{}}: {text}");
    assert!(data.get("calls").is_none(), "no calls:[]: {text}");
    assert!(
        data["files"][0].get("related").is_none(),
        "no related:[]: {text}"
    );
    assert_eq!(data["files"][0]["path"], ".env", "the row stays: {text}");
    // A container that was empty before the pass is not its to remove.
    assert_eq!(
        data["kept"],
        json_fixture(r#"{"empty": {}, "none": []}"#),
        "{text}"
    );
    assert!(
        text.contains("3 ghGetFileContent read row(s) were dropped from leads"),
        "{text}"
    );
}

#[test]
fn the_opt_in_keeps_credential_name_leads_never_sensitive_directories() {
    let mut data = row();
    drop_withheld_reads(&mut data, &policy(true));
    let text = data.to_string();
    assert_eq!(
        data["files"][0]["related"].as_array().map(Vec::len),
        Some(2),
        "{text}"
    );
    assert!(!text.contains(".ssh/config"), "{text}");
    assert!(
        text.contains("2 ghGetFileContent read row(s) were dropped from leads"),
        "{text}"
    );
}
