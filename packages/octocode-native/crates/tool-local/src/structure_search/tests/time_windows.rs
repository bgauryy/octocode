//! A duration back from now in `modifiedAt` or `accessedAt` resolves once
//! per query: the listing applies the absolute bound, the response states
//! it in `window`, and a window inverted once resolved is refused.
use super::*;

const DAY: i64 = 86_400;

#[test]
fn a_duration_window_lists_by_its_resolved_bound_and_states_it() {
    let root = Fixture::new();
    let now = std::time::SystemTime::now();
    for (name, age) in [("fresh.txt", 3_600u64), ("stale.txt", 3 * 86_400)] {
        let path = root.0.join(name);
        std::fs::write(&path, "x\n").expect("fixture");
        std::fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|file| file.set_modified(now - std::time::Duration::from_secs(age)))
            .expect("mtime");
    }
    let before = octocode_runtime_base::civil_date::unix_secs();
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"entryType":"file","modifiedAt":{"from":"-P1D"}}),
    )
    .expect("listing");
    let after = octocode_runtime_base::civil_date::unix_secs();
    let names = paths(&out)
        .into_iter()
        .map(|path| path.rsplit('/').next().unwrap_or(&path).to_owned())
        .collect::<Vec<_>>();
    assert_eq!(names, ["fresh.txt"], "{out}");
    let from = out["window"]["modifiedAt"]["from"]
        .as_str()
        .unwrap_or_else(|| panic!("the resolved bound is stated: {out}"));
    let (seconds, _) =
        octocode_runtime_base::civil_date::parse_rfc3339(from).expect("an RFC 3339 bound");
    assert!(
        before - DAY <= seconds && seconds <= after - DAY,
        "{from} is a day before the call"
    );
    assert!(out["window"].get("accessedAt").is_none(), "{out}");

    // A window of absolute bounds states none.
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"modifiedAt":{"from":"2020-01-01T00:00:00Z"}}),
    )
    .expect("listing");
    assert!(out.get("window").is_none(), "{out}");
}

#[test]
fn a_window_inverted_once_resolved_is_refused() {
    let root = Fixture::new();
    for window in [
        json!({"from": "-PT1H", "to": "-P1D"}),
        json!({"from": "2026-10-24T10:00:00Z", "to": "2026-10-23T10:00:00Z"}),
    ] {
        let error = run(
            &root.0,
            json!({"operation":"files","path":root.0,"accessedAt":window}),
        )
        .expect_err("an inverted window");
        assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
        assert!(
            format!("{error:?}").contains("accessedAt.to: to must be at least from"),
            "{error:?}"
        );
    }
}
