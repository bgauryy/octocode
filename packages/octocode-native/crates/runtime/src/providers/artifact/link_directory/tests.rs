//! Placing a source link's directory: the registry's own, the pinned ref, or a
//! ref the repository confirms.
use super::super::http::RegistryClient;
use super::super::test_http::{KnownTags, RouteHttp, test_budget};
use super::super::{CommitCheck, RepositoryStatus};
use super::{ArtifactItem, settle};
use octocode_contract_types::{ArtifactType, ErrorCode};

const REPOSITORY: &str = "https://github.com/o/r";

/// A row the registry gave `link` for, with the directory and ref it states.
fn row(link: &str, directory: Option<&str>, source_ref: Option<&str>) -> ArtifactItem {
    let mut item = ArtifactItem::new(ArtifactType::Pypi, "pkg".into(), String::new());
    item.repository_directory = directory.map(str::to_owned);
    item.source_ref = source_ref.map(str::to_owned);
    item.set_source_link(link);
    item
}

/// The rows after their links are settled against `tags` (`None`: no GitHub
/// access), under `budget`.
async fn settle_all(
    mut rows: Vec<ArtifactItem>,
    tags: Option<&KnownTags>,
    budget: &octocode_io::budget::RequestBudget,
) -> Result<Vec<ArtifactItem>, super::super::ArtifactError> {
    let http = RouteHttp::new();
    let client = RegistryClient {
        tags: tags.map(|tags| tags as &dyn super::super::ReleaseTags),
        ..RegistryClient::new(&http, budget)
    };
    settle(&mut rows, &client).await?;
    Ok(rows)
}

/// The row after its link is settled against `tags`.
async fn settled(item: ArtifactItem, tags: Option<&KnownTags>) -> ArtifactItem {
    let rows = settle_all(vec![item], tags, &test_budget())
        .await
        .expect("links settle");
    rows.into_iter().next().expect("one row")
}

/// A link alone cannot place where a ref ends, so a row never takes a directory
/// from it before it is settled.
#[test]
fn a_source_link_alone_places_no_directory() {
    let item = row(&format!("{REPOSITORY}/tree/main/pkg"), None, None);
    assert_eq!(item.repository.as_deref(), Some(REPOSITORY));
    assert_eq!(item.repository_directory, None);
    assert!(item.disclosures.is_empty());
}

/// A ref may contain `/`: when the row pins a ref the link starts with, the
/// directory is what the link has below that whole ref, and nothing is asked.
#[tokio::test]
async fn a_slash_ref_matching_the_pinned_ref_splits_the_link() {
    for (pinned, link, directory) in [
        ("v2/main", "tree/v2/main/pkg/a", Some("pkg/a")),
        ("users/x", "blob/users/x/pkg/README.md", Some("pkg")),
        ("release/1.x", "tree/release/1.x/pkg", Some("pkg")),
        ("main", "tree/main/pkg", Some("pkg")),
        ("v2/main", "tree/v2/main", None),
    ] {
        let tags = KnownTags::new(Vec::new());
        let item = settled(
            row(&format!("{REPOSITORY}/{link}"), None, Some(pinned)),
            Some(&tags),
        )
        .await;
        assert_eq!(item.repository_directory.as_deref(), directory, "{link}");
        assert!(
            item.disclosures.is_empty(),
            "{link}: {:?}",
            item.disclosures
        );
        assert!(tags.seen().is_empty(), "{link}: {:?}", tags.seen());
    }
}

/// Without a pinned ref the first segment is the ref only when the repository
/// confirms it: one lookup, and its answer decides.
#[tokio::test]
async fn a_first_segment_the_repository_confirms_ends_the_ref() {
    let tags = KnownTags::new(vec!["o/r@main"]);
    let item = settled(
        row(&format!("{REPOSITORY}/tree/main/pkg/a"), None, None),
        Some(&tags),
    )
    .await;
    assert_eq!(item.repository_directory.as_deref(), Some("pkg/a"));
    assert!(item.disclosures.is_empty());
    assert_eq!(tags.seen(), ["o/r@main"]);
}

/// A link below a first segment nothing confirms (`users` of `users/x`, or a
/// pinned ref the link does not start with) gives no directory, and the row
/// says so: no guess, whether the repository denies the ref or cannot be asked.
#[tokio::test]
async fn an_unconfirmed_multi_segment_link_gives_no_directory_and_a_warning() {
    let link = format!("{REPOSITORY}/tree/users/x/pkg");
    for pinned in [None, Some("0123456789abcdef0123456789abcdef01234567")] {
        let tags = KnownTags::new(Vec::new());
        let item = settled(row(&link, None, pinned), Some(&tags)).await;
        assert_eq!(item.repository_directory, None, "{pinned:?}");
        assert_eq!(item.disclosures.len(), 1, "{:?}", item.disclosures);
        assert!(
            item.disclosures[0].contains(&link),
            "{:?}",
            item.disclosures
        );
        assert_eq!(tags.seen(), ["o/r@users"], "{pinned:?}");
    }
    assert!(
        settled(row(&link, None, None), Some(&KnownTags::new(Vec::new())))
            .await
            .disclosures[0]
            .contains("does not confirm users")
    );
}

/// A check that never ran is unknown, not a "no" from the repository: the
/// row says it could not check and why, whether GitHub is unavailable or gave
/// no answer.
#[tokio::test]
async fn an_unchecked_link_is_disclosed_as_unchecked_with_its_reason() {
    let link = format!("{REPOSITORY}/tree/users/x/pkg");
    let unanswered = KnownTags::always(None);
    for (tags, reason) in [
        (None, "not available"),
        (Some(&unanswered), "gave no answer"),
    ] {
        let item = settled(row(&link, None, None), tags).await;
        assert_eq!(item.repository_directory, None, "{reason}");
        assert_eq!(item.disclosures.len(), 1, "{:?}", item.disclosures);
        let text = &item.disclosures[0];
        assert!(text.contains("could not be checked"), "{text}");
        assert!(text.contains(reason), "{text}");
        assert!(!text.contains("does not confirm"), "{text}");
    }
}

/// A repository GitHub does not have is said on the row the way every other
/// check says it, not dropped as an unconfirmed ref.
#[tokio::test]
async fn a_missing_repository_is_noted_on_the_row() {
    let tags = KnownTags::new(Vec::new()).with_repository(CommitCheck::RepositoryMissing);
    let item = settled(
        row(&format!("{REPOSITORY}/tree/main/pkg"), None, None),
        Some(&tags),
    )
    .await;
    assert_eq!(item.repository_directory, None);
    assert_eq!(item.repository_status, Some(RepositoryStatus::Missing));
}

/// A cancelled call fails; it is not a row that could not be checked.
#[tokio::test]
async fn a_cancelled_call_fails_instead_of_leaving_rows_unchecked() {
    let cancellation = octocode_io::cancellation::Cancellation::new();
    cancellation.cancel();
    let budget = octocode_io::budget::RequestBudget::new(
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        cancellation,
        10_000_000,
    );
    let tags = KnownTags::always(None);
    let error = settle_all(
        vec![row(&format!("{REPOSITORY}/tree/users/x/pkg"), None, None)],
        Some(&tags),
        &budget,
    )
    .await
    .expect_err("cancelled");
    assert_eq!(error.code, ErrorCode::Cancelled);
}

/// Rows of a page that share a repository and a first segment share one
/// lookup; a different first segment is its own.
#[tokio::test]
async fn rows_sharing_a_repository_and_first_segment_cost_one_lookup() {
    let tags = KnownTags::new(vec!["o/r@main", "o/other@main"]);
    let rows = vec![
        row(&format!("{REPOSITORY}/tree/main/gems/a"), None, None),
        row(&format!("{REPOSITORY}/tree/main/gems/b"), None, None),
        row(
            &format!("{REPOSITORY}/blob/main/gems/c/README.md"),
            None,
            None,
        ),
        row(&format!("{REPOSITORY}/tree/users/x/gems/d"), None, None),
        row("https://github.com/o/other/tree/main/gems/e", None, None),
    ];
    let rows = settle_all(rows, Some(&tags), &test_budget())
        .await
        .expect("settled");
    let directories: Vec<_> = rows
        .iter()
        .map(|row| row.repository_directory.as_deref())
        .collect();
    assert_eq!(
        directories,
        [
            Some("gems/a"),
            Some("gems/b"),
            Some("gems/c"),
            None,
            Some("gems/e")
        ]
    );
    let mut seen = tags.seen();
    seen.sort();
    assert_eq!(seen, ["o/other@main", "o/r@main", "o/r@users"]);
}

/// A directory the registry states is the directory: the link is not read,
/// whatever ref the row pins and whatever the repository would confirm.
#[tokio::test]
async fn an_explicit_registry_directory_always_wins() {
    let tags = KnownTags::new(vec!["o/r@release"]);
    for link in [
        "tree/release/1.x/other",
        "tree/main/other",
        "blob/users/x/other/a.rs",
    ] {
        let item = settled(
            row(
                &format!("{REPOSITORY}/{link}"),
                Some("packages/pkg"),
                Some("release/1.x"),
            ),
            Some(&tags),
        )
        .await;
        assert_eq!(
            item.repository_directory.as_deref(),
            Some("packages/pkg"),
            "{link}"
        );
        assert!(item.disclosures.is_empty(), "{link}");
    }
    assert!(tags.seen().is_empty(), "{:?}", tags.seen());
}
