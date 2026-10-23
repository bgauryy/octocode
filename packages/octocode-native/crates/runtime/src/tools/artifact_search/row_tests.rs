//! Row shaping: GitHub coordinates beside the repository URL and repository homepages.
use super::{ArtifactItem, ArtifactType, drop_repository_homepage, name_github_repository};
use serde_json::json;

/// A GitHub repository URL names its owner and repo beside itself; any
/// other host, or no repository, adds nothing.
#[test]
fn github_rows_name_owner_and_repo_after_the_repository_url() {
    let mut row = json!({"name":"ajv","repository":"https://github.com/ajv-validator/ajv.git","version":"8.0.0"});
    name_github_repository(&mut row, false);
    assert_eq!(
        row.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["name", "repository", "owner", "repo", "version"]
    );
    assert_eq!(row["owner"], "ajv-validator");
    assert_eq!(row["repo"], "ajv");
    let mut other = json!({"name":"x","repository":"https://gitlab.com/g/x"});
    name_github_repository(&mut other, false);
    assert!(other.get("owner").is_none() && other.get("repo").is_none());
    let mut none = json!({"name":"x"});
    name_github_repository(&mut none, false);
    assert_eq!(none, json!({"name":"x"}));
}

/// A repository URL that is exactly `https://github.com/{owner}/{repo}`
/// says nothing `owner` and `repo` do not: it is dropped (kept with
/// `details`). A URL that says more (a ref, a `.git` suffix) stays.
#[test]
fn a_repository_url_restating_owner_and_repo_is_dropped() {
    let mut row = json!({"name":"joi","repository":"https://github.com/hapijs/joi","version":"1"});
    name_github_repository(&mut row, false);
    assert_eq!(
        row,
        json!({"name":"joi","owner":"hapijs","repo":"joi","version":"1"})
    );
    assert_eq!(
        row.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["name", "owner", "repo", "version"]
    );
    let mut details = json!({"name":"joi","repository":"https://github.com/hapijs/joi"});
    name_github_repository(&mut details, true);
    assert_eq!(details["repository"], "https://github.com/hapijs/joi");
    let mut pinned =
        json!({"name":"convict","repository":"https://github.com/mozilla/node-convict.git#master"});
    name_github_repository(&mut pinned, false);
    assert_eq!(
        pinned["repository"],
        "https://github.com/mozilla/node-convict.git#master"
    );
    assert_eq!(
        (&pinned["owner"], &pinned["repo"]),
        (&json!("mozilla"), &json!("node-convict"))
    );
}

fn row(homepage: &str, repository: &str) -> ArtifactItem {
    let mut item = ArtifactItem::new(ArtifactType::Npm, "x".into(), "https://r/x".into());
    item.homepage = Some(homepage.into());
    item.repository = Some(repository.into());
    item.description = Some("d".into());
    item
}

#[test]
fn only_a_homepage_that_is_the_repository_page_is_dropped() {
    let mut same = row("https://github.com/o/x#readme", "https://github.com/o/x");
    drop_repository_homepage(&mut same);
    assert_eq!(same.homepage, None);
    // A tree page of the repository, or a `.git` repository URL, is the
    // repository too.
    for (homepage, repository) in [
        (
            "https://github.com/DT/DT/tree/master/types/node",
            "https://github.com/DT/DT",
        ),
        (
            "https://github.com/Seldaek/monolog",
            "https://github.com/Seldaek/monolog.git",
        ),
    ] {
        let mut same = row(homepage, repository);
        drop_repository_homepage(&mut same);
        assert_eq!(same.homepage, None, "{homepage}");
    }
    let mut distinct = row("https://zod.dev", "https://github.com/o/x");
    drop_repository_homepage(&mut distinct);
    assert_eq!(distinct.homepage.as_deref(), Some("https://zod.dev"));
    // The registry URL is verbose: the verbose stage drops it by default.
    assert!(
        crate::response::extras::all_paths(crate::id::ToolId::ArtifactSearch)
            .contains(&"results[].data.artifacts[].registryUrl")
    );
}

#[test]
fn rows_carry_no_type_and_start_with_the_name() {
    let encoded =
        serde_json::to_value(row("https://zod.dev", "https://github.com/o/x")).expect("row");
    let keys = encoded.as_object().expect("row").keys().collect::<Vec<_>>();
    assert_eq!(
        keys.first().map(|key| key.as_str()),
        Some("name"),
        "{encoded}"
    );
    assert!(!keys.iter().any(|key| *key == "type"), "{encoded}");
}
