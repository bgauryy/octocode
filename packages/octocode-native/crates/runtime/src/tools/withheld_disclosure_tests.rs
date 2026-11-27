//! How the tools' skipped-path rows disclose what path policy withheld
//! (`octocode_runtime_base::policy::discovery`): by the directories that hold
//! it, never the entries.

/// A disclosure of withheld entries names the directory that holds them, by
/// its absolute path like every skipped path (the response spells them all
/// from the workspace root), and never an entry itself (B143).
#[test]
fn a_directory_holding_withheld_files_is_not_disclosed_as_the_file() {
    use octocode_runtime_base::policy::discovery::Withheld;
    use octocode_runtime_base::policy::discovery::withheld::WithheldKind;
    use octocode_tools_shared::leads::skipped::unlifted;
    let root = tempfile::tempdir().expect("root");
    let deploy = root.path().join("deploy");
    std::fs::create_dir_all(&deploy).expect("dirs");
    let (env, key) = (deploy.join(".env"), root.path().join("id_rsa"));
    std::fs::write(&env, "A=1\n").expect("credential file");
    std::fs::write(&key, "k\n").expect("credential file");
    let mut withheld = Withheld::default();
    withheld.record(&env);
    withheld.record(&key);
    let walked = root.path().to_string_lossy().into_owned();
    let entries = unlifted(&walked, &withheld, Default::default());
    // One entry per (reason, directory); the root itself only for what sits
    // directly in it.
    let holders = |path: &str| {
        entries
            .iter()
            .filter(|entry| entry.path == path)
            .collect::<Vec<_>>()
    };
    let held_at = deploy.to_string_lossy().into_owned();
    for (directory, held) in [(held_at.as_str(), ".env"), (walked.as_str(), "id_rsa")] {
        let at = holders(directory);
        assert_eq!(at.len(), 1, "{entries:?}");
        assert_eq!(at[0].count, 1);
        assert_eq!(
            at[0].reason.as_deref(),
            Some(WithheldKind::CredentialFile.reason().as_ref())
        );
        assert!(
            entries.iter().all(|entry| !entry.path.ends_with(held)),
            "a withheld file is never named: {entries:?}"
        );
    }
    assert_eq!(
        entries.iter().map(|entry| entry.count).sum::<usize>(),
        withheld.total()
    );
    // The default view keeps each holder its own entry too: a withheld entry
    // names where its files sit, never a common ancestor of two holders.
    let shown = octocode_tools_shared::leads::skipped::skipped_paths(entries, false, false)
        .expect("skipped paths")
        .expect("some skipped");
    let mut paths = shown
        .entries
        .iter()
        .map(|entry| (entry.path.as_str().to_owned(), entry.count))
        .collect::<Vec<_>>();
    paths.sort();
    let mut expected_paths = vec![(walked.clone(), None), (held_at.clone(), None)];
    expected_paths.sort();
    assert_eq!(paths, expected_paths, "{shown:?}");
    // The detailed view still exposes both holder directories exactly once,
    // never the withheld files themselves, with the same total count.
    let detailed = octocode_tools_shared::leads::skipped::skipped_paths(
        unlifted(&walked, &withheld, Default::default()),
        true,
        false,
    )
    .expect("skipped paths")
    .expect("some skipped");
    assert_ne!(detailed.sampled, Some(true));
    let mut shown = serde_json::to_value(detailed.entries).expect("holder paths");
    shown
        .as_array_mut()
        .expect("entries")
        .sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let mut expected = serde_json::json!([
        {"cause": "withheld", "path": walked, "reason": WithheldKind::CredentialFile.reason()},
        {"cause": "withheld", "path": held_at, "reason": WithheldKind::CredentialFile.reason()}
    ]);
    expected
        .as_array_mut()
        .expect("entries")
        .sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    assert_eq!(shown, expected);
}
