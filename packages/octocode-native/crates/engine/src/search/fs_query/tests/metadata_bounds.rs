use super::*;
use std::fs::FileTimes;
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn file_size_bounds_are_inclusive_and_never_match_directories() {
    let root = TmpDir::new();
    fs::create_dir(root.join("directory")).expect("directory");
    for (name, content) in [("zero", ""), ("five", "12345"), ("six", "123456")] {
        fs::write(root.join(name), content).expect("file");
    }
    for (size_min, size_max, expected) in [
        (Some(0), Some(0), vec!["zero"]),
        (Some(5), Some(5), vec!["five"]),
        (Some(5), None, vec!["five", "six"]),
        (None, Some(5), vec!["five", "zero"]),
    ] {
        let found = query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            size_min,
            size_max,
            ..Default::default()
        })
        .expect("query");
        let names = found
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, expected, "{size_min:?}..={size_max:?}");
    }
}

#[test]
fn metadata_dates_use_fixed_inclusive_instants() {
    let root = TmpDir::new();
    let at = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    for (name, seconds) in [("before", 0), ("at", 1), ("after", 2)] {
        let file = File::create(root.join(name)).expect("file");
        let time = at + Duration::from_secs(seconds);
        file.set_times(FileTimes::new().set_modified(time).set_accessed(time))
            .expect("metadata times");
    }
    let boundary = at + Duration::from_secs(1);
    for accessed in [false, true] {
        let found = query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            modified_from: (!accessed).then_some(boundary),
            modified_to: (!accessed).then_some(boundary),
            accessed_from: accessed.then_some(boundary),
            accessed_to: accessed.then_some(boundary),
            ..Default::default()
        })
        .expect("query");
        assert_eq!(
            found
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["at"]
        );
    }
}

#[test]
fn reversed_metadata_bounds_fail_before_the_walk() {
    let at = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    for bounds in [
        FileSystemQueryOptions {
            size_min: Some(6),
            size_max: Some(5),
            ..Default::default()
        },
        FileSystemQueryOptions {
            modified_from: Some(at + Duration::from_secs(1)),
            modified_to: Some(at),
            ..Default::default()
        },
        FileSystemQueryOptions {
            accessed_from: Some(at + Duration::from_secs(1)),
            accessed_to: Some(at),
            ..Default::default()
        },
    ] {
        let error = query_file_system(
            FileSystemQueryOptions {
                path: "/definitely/missing/octocode-root".to_owned(),
                ..bounds
            },
            &|_, _, _| true,
            &|| false,
        )
        .expect_err("reversed range");
        assert_eq!(error.kind(), &ErrorKind::InvalidInput);
    }
}
