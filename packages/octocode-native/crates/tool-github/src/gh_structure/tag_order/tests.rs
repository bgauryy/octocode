use super::*;

/// ripgrep's tags: the invariant is that each prefix's tags are one
/// contiguous run, in descending version order, and that unversioned
/// tags come last: `termcolor-1.4.1` never lands between `13.0.0` and
/// `0.10.0` by its number.
#[test]
fn each_prefix_is_one_run_in_version_order() {
    let tags = [
        "wincolor-0.1.6",
        "termcolor-1.4.1",
        "nightly",
        "ignore-0.4.33",
        "grep-0.3.2",
        "15.1.0",
        "15.2.0",
        "15.0.0-rc.1",
        "15.0.0-rc.10",
        "15.0.0",
        "13.0.0",
        "0.10.0",
        "rmcp-v0.8.5",
        "rmcp-v0.9.0",
        "termcolor-1.4.0",
        "v14.1.1",
    ];
    let listed = sorted(&tags);
    assert_eq!(listed.len(), tags.len());
    let mut runs: Vec<(Option<String>, Vec<&String>)> = Vec::new();
    for name in &listed {
        let prefix = TagVersion::of(name).prefix().map(str::to_owned);
        match runs.last_mut() {
            Some((last, run)) if *last == prefix => run.push(name),
            _ => runs.push((prefix, vec![name])),
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for (prefix, run) in &runs {
        assert!(seen.insert(prefix), "{prefix:?} is split: {listed:?}");
        for pair in run.windows(2) {
            assert!(
                TagVersion::of(pair[0]) >= TagVersion::of(pair[1]),
                "{pair:?} out of order in {listed:?}"
            );
        }
    }
    assert_eq!(runs.last().and_then(|(prefix, _)| prefix.as_deref()), None);
    assert_eq!(runs.len(), 6 + 1);
}

/// A monorepo's package tags never lead its own releases, whatever
/// their numbers: the first page answers "the repository's newest
/// release" (vite: `v8.3.8` before `create-vite@9.3.4`).
#[test]
fn the_repositorys_own_releases_lead_package_groups() {
    let listed = sorted(&[
        "create-vite@9.3.4",
        "v8.3.7",
        "plugin-legacy@7.0.0",
        "create-vite@9.3.3",
        "v8.3.8",
        "v8.3.8-beta.1",
        "nightly",
    ]);
    assert_eq!(
        listed,
        [
            "v8.3.8",
            "v8.3.8-beta.1",
            "v8.3.7",
            "create-vite@9.3.4",
            "create-vite@9.3.3",
            "plugin-legacy@7.0.0",
            "nightly"
        ]
    );
}

/// One version under several prefixes keeps GitHub's
/// reverse-lexicographic order; so do unversioned tags.
#[test]
fn ties_keep_reverse_lexicographic_order() {
    let mut names = ["a-1.0.0", "c-1.0.0", "b-1.0.0", "beta", "alpha"].map(str::to_owned);
    newest_first(&mut names, "repo");
    assert_eq!(names, ["c-1.0.0", "b-1.0.0", "a-1.0.0", "beta", "alpha"]);
}

fn sorted(names: &[&str]) -> Vec<String> {
    let mut names = names
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    newest_first(&mut names, "repo");
    names
}

/// A dated or lone-number tag (`translation/20190712`, `v14`) is not a
/// release: within a group dotted versions lead, descending, whatever
/// number the others start with.
#[test]
fn dotted_versions_lead_over_dates_and_lone_numbers() {
    let listed = sorted(&["v14", "1.145.0", "1.145.1", "v2", "latest"]);
    assert_eq!(listed, ["1.145.1", "1.145.0", "v14", "v2", "latest"]);
    // A date stamp in its own prefix group stays one run.
    let listed = sorted(&[
        "translation/20190712",
        "1.145.0",
        "translation/20172701.01",
        "v14",
    ]);
    assert_eq!(
        listed,
        [
            "1.145.0",
            "v14",
            "translation/20190712",
            "translation/20172701.01"
        ]
    );
}

/// Without any dotted version, lone numbers still order newest first.
#[test]
fn lone_number_tags_order_among_themselves() {
    assert_eq!(
        sorted(&["v1", "v10", "v2", "beta"]),
        ["v10", "v2", "v1", "beta"]
    );
}
