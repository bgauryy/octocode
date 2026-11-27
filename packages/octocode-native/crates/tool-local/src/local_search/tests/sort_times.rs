//! A time order states each file's time: the rank is the evidence, so a
//! file listed by `modifiedAt` carries the time it was ranked by.
use super::*;

/// Rows of `fields` over three files whose modification times are a day
/// apart, the newest written first.
fn listed(fields: serde_json::Value) -> serde_json::Value {
    let root = tempfile::tempdir().expect("fixture");
    let day = std::time::Duration::from_secs(86_400);
    let base = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    for (name, days) in [("c.txt", 2u32), ("a.txt", 0), ("b.txt", 1)] {
        let path = root.path().join(name);
        fs::write(&path, "needle\n").expect("fixture");
        fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|file| file.set_modified(base + day * days))
            .expect("mtime");
    }
    let (policy, security) = policy_for(root.path());
    let mut fields = fields;
    fields["path"] = root.path().to_string_lossy().into_owned().into();
    fields["matchString"] = serde_json::json!(["needle"]);
    search_in(&ls_query(fields, None), &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("row")
}

#[test]
fn a_time_order_states_each_files_time_in_its_order() {
    for view in ["files", "content", "countLines"] {
        let row = listed(serde_json::json!({
            "view": view,
            "sort": {"by": "modifiedAt", "order": "desc"},
        }));
        let files = row["files"].as_array().expect("files");
        let times = files
            .iter()
            .map(|file| {
                file["modifiedAt"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{view}: {file}"))
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(times.len(), 3, "{view}: {row}");
        assert!(
            times.windows(2).all(|pair| pair[0] > pair[1]),
            "{view}: newest first, each with its time: {times:?}"
        );
        assert_eq!(
            times.last().map(String::as_str),
            Some(octocode_runtime_base::civil_date::iso8601_secs(1_700_000_000).as_str()),
            "{view}: the oldest file's own time"
        );
    }
    // Any other order states no time.
    let row = listed(serde_json::json!({"view": "files"}));
    for file in row["files"].as_array().expect("files") {
        for key in ["modifiedAt", "accessedAt", "createdAt"] {
            assert!(file.get(key).is_none(), "{file}");
        }
    }
}
