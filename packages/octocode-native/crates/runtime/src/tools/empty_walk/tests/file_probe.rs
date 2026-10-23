//! [`FileProbe`]: one tagged walk answers each relaxed walk, within the
//! request's globs, past files the request reached, and cut when stopped.

use super::*;

/// Files the request's own walk reached are not behind a skipped path: a probe
/// that is told the walk counts only what relaxing adds.
#[test]
fn a_probe_counts_only_files_the_requested_walk_did_not_reach() {
    let root = repo("", &["src", ".cfg"]);
    let all = typescript_probe(&root.0)
        .find(DEFAULTS.lifting(&[Cause::Hidden]))
        .expect("probe");
    assert_eq!(all.count, 2, "both directories, with no baseline");
    let behind = typescript_probe(&root.0)
        .beyond(DEFAULTS)
        .find(DEFAULTS.lifting(&[Cause::Hidden]))
        .expect("probe");
    assert_eq!(behind.count, 1, "only the hidden directory's file");
    assert!(
        behind.sample[0].starts_with(root.0.join(".cfg")),
        "{behind:?}"
    );
}

/// A walk that skips paths but whose probe finds nothing new is absent: the
/// diagnosis never claims a skipped cause for files the scan reached.
#[test]
fn nothing_behind_skipped_paths_is_absent_even_when_files_were_reached() {
    let root = repo("", &["src"]);
    let probe = typescript_probe(&root.0).beyond(DEFAULTS);
    let verdict = diagnose(&root.0, DEFAULTS, |walk| probe.find(walk)).expect("probe");
    assert!(matches!(verdict, Verdict::Absent), "{verdict:?}");
}

/// The probe applies the request's `include` and `exclude` globs, like the
/// scan it stands for.
#[test]
fn a_probe_stays_within_the_request_globs() {
    let root = repo("", &[".github", ".spin", "src"]);
    let include = vec![".github/**".to_owned()];
    let probe = typescript_probe(&root.0)
        .within(Globs {
            include: &include,
            exclude: &[],
        })
        .beyond(DEFAULTS);
    let found = probe
        .find(DEFAULTS.lifting(&[Cause::Hidden]))
        .expect("probe");
    assert_eq!(found.count, 1, "{found:?}");
    assert!(
        found.sample[0].starts_with(root.0.join(".github")),
        "{found:?}"
    );
    let exclude = vec!["**/.github/**".to_owned()];
    let excluded = typescript_probe(&root.0).within(Globs {
        include: &[],
        exclude: &exclude,
    });
    let found = excluded
        .find(DEFAULTS.lifting(&[Cause::Hidden]))
        .expect("probe");
    assert_eq!(found.count, 2, "{found:?}");
}

/// Every relaxed walk a diagnosis asks about is answered from one fully
/// lifted walk: each file is admitted once, however many walks are probed.
#[test]
fn one_walk_answers_every_relaxed_walk() {
    let root = repo(
        "gen/\nnode_modules/\n",
        &["gen", ".cfg", "node_modules/pkg", "src"],
    );
    let admitted = Cell::new(0);
    let permits = |_: &Path| {
        admitted.set(admitted.get() + 1);
        true
    };
    let keep = |path: &Path| path.extension().is_some_and(|ext| ext == "ts");
    let probe = FileProbe::new(&root.0, 1_000, &permits, &keep).beyond(DEFAULTS);
    let verdict = diagnose(&root.0, DEFAULTS, |walk| probe.find(walk)).expect("probe");
    let Verdict::Skipped(diagnosis) = verdict else {
        panic!("{verdict:?}")
    };
    assert_eq!(
        diagnosis.causes(),
        [Cause::Ignored, Cause::Hidden, Cause::DefaultExcluded]
    );
    assert_eq!(diagnosis.found().count, 3);
    // `.gitignore`, `visible.md` and the four `needle.ts` files, once each.
    assert_eq!(admitted.get(), 6);
}

/// A stopped probe is cut, never an error: absence there is unproven.
#[test]
fn a_stopped_probe_is_cut() {
    let root = repo("", &[".cfg"]);
    let stop = || true;
    let found = typescript_probe(&root.0)
        .stopping(&stop)
        .find(DEFAULTS.lifting(&[Cause::Hidden]))
        .expect("probe");
    assert!(found.cut && found.count == 0, "{found:?}");
}
