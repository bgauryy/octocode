use super::{Cause, FileProbe, Globs, Hits, Verdict, Walk, diagnose, ignore_rule, lift};
use crate::tools::test_support::Fixture;
use std::cell::Cell;
use std::path::{Path, PathBuf};

mod file_probe;

const DEFAULTS: Walk = Walk {
    ignored: false,
    hidden: false,
    default_excludes: true,
};

/// `root` with a git marker, the `.gitignore` text, and one `needle.ts` under
/// each of `dirs`.
fn repo(gitignore: &str, dirs: &[&str]) -> Fixture {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), gitignore).expect("gitignore");
    std::fs::write(root.0.join("visible.md"), "x\n").expect("visible");
    for dir in dirs {
        std::fs::create_dir_all(root.0.join(dir)).expect("dir");
        std::fs::write(root.0.join(dir).join("needle.ts"), "x\n").expect("file");
    }
    root
}

/// [`repo`]'s layout outside any git work tree, where a default-excluded
/// directory name is pruned by its name alone.
fn plain(dirs: &[&str]) -> Fixture {
    let root = Fixture::new();
    std::fs::write(root.0.join("visible.md"), "x\n").expect("visible");
    for dir in dirs {
        std::fs::create_dir_all(root.0.join(dir)).expect("dir");
        std::fs::write(root.0.join(dir).join("needle.ts"), "x\n").expect("file");
    }
    root
}

/// The probe a file-walking tool supplies: `.ts` files under the relaxed walk.
fn typescript(root: &Path) -> impl FnMut(Walk) -> Result<Hits, String> + '_ {
    let probe = typescript_probe(root);
    move |walk| probe.find(walk)
}

fn typescript_probe(root: &Path) -> FileProbe<'_> {
    FileProbe::new(root, 1_000, &|_| true, &|path| {
        path.extension().is_some_and(|ext| ext == "ts")
    })
}

fn skipped(root: &Path, walk: Walk) -> super::Diagnosis {
    match diagnose(root, walk, typescript(root)).expect("probe") {
        Verdict::Skipped(diagnosis) => diagnosis,
        other => panic!("expected skipped matches: {other:?}"),
    }
}

#[test]
fn each_cause_alone_is_named_and_lifted_alone() {
    for (gitignore, dir, cause, reason) in [
        ("gen/\n", "gen", Cause::Ignored, ".gitignore:1 'gen/'"),
        ("", ".cfg", Cause::Hidden, "hidden .cfg/"),
        (
            "",
            "node_modules/pkg",
            Cause::DefaultExcluded,
            "node_modules/",
        ),
    ] {
        // Inside a git work tree a listed name is pruned only where git
        // ignores it, so a default-excluded directory alone is one outside.
        let root = if cause == Cause::DefaultExcluded {
            plain(&[dir])
        } else {
            repo(gitignore, &[dir])
        };
        let diagnosis = skipped(&root.0, DEFAULTS);
        assert_eq!(diagnosis.causes(), [cause], "{dir}");
        assert_eq!(diagnosis.because(), reason, "{dir}");
        let mut lead = serde_json::json!({"path": "."});
        lift(&mut lead, diagnosis.causes());
        let (field, value) = cause.flag();
        assert_eq!(
            lead,
            serde_json::json!({"path": ".", field: value}),
            "{dir}"
        );
    }
}

#[test]
fn the_entry_is_the_topmost_skipped_path() {
    let root = repo("gen/\n", &["gen/deep/er"]);
    let diagnosis = skipped(&root.0, DEFAULTS);
    assert_eq!(diagnosis.skips()[0].entry, root.0.join("gen"));
    let root = repo("", &["pkg/.cfg/inner"]);
    let diagnosis = skipped(&root.0, DEFAULTS);
    assert_eq!(diagnosis.skips()[0].entry, root.0.join("pkg/.cfg"));
    assert_eq!(diagnosis.because(), "hidden pkg/.cfg/");
}

/// A directory two causes hide needs both lifted; one alone finds nothing.
#[test]
fn a_match_behind_two_causes_lifts_exactly_those_two() {
    for (gitignore, dir, causes) in [
        (".gen/\n", ".gen", vec![Cause::Ignored, Cause::Hidden]),
        (
            "build/\n",
            "build/out",
            vec![Cause::Ignored, Cause::DefaultExcluded],
        ),
        (
            "",
            ".cache/node_modules",
            vec![Cause::Hidden, Cause::DefaultExcluded],
        ),
    ] {
        let root = if causes.contains(&Cause::Ignored) {
            repo(gitignore, &[dir])
        } else {
            plain(&[dir])
        };
        let diagnosis = skipped(&root.0, DEFAULTS);
        assert_eq!(diagnosis.causes(), causes, "{dir}");
        assert!(!diagnosis.because().is_empty(), "{dir}");
    }
}

/// Files behind a dot path alone, and one behind a dot path and an ignore
/// rule together: the smallest set reaches only some of them, so the larger
/// set is probed too, and the hint and lead name both causes.
#[test]
fn a_larger_cause_set_is_probed_while_files_stay_unreached() {
    let root = repo(".both/\n", &[".cfg", ".cfg/deep", ".both"]);
    let diagnosis = skipped(&root.0, DEFAULTS);
    assert_eq!(diagnosis.causes(), [Cause::Ignored, Cause::Hidden]);
    assert_eq!(diagnosis.found().count, 3, "{diagnosis:?}");
    let reasons = diagnosis.because();
    assert!(reasons.contains("hidden .cfg/"), "{reasons}");
    assert!(
        reasons.contains(".gitignore:1 '.both/'") || reasons.contains("ignored"),
        "{reasons}"
    );
    let mut lead = serde_json::json!({"path": "."});
    lift(&mut lead, diagnosis.causes());
    assert_eq!(
        lead,
        serde_json::json!({"path": ".", "includeIgnored": true, "includeHidden": true})
    );
}

/// Matches behind different causes in different directories lift each
/// cause that holds some, and never one that holds none.
#[test]
fn separate_causes_are_unioned_and_an_empty_cause_stays_out() {
    let root = repo("gen/\n", &["gen", ".cfg"]);
    std::fs::create_dir_all(root.0.join("node_modules/pkg")).expect("pruned");
    std::fs::write(root.0.join("node_modules/pkg/other.md"), "x\n").expect("other");
    let diagnosis = skipped(&root.0, DEFAULTS);
    assert_eq!(diagnosis.causes(), [Cause::Ignored, Cause::Hidden]);
    assert_eq!(diagnosis.found().count, 2);
    let reasons = diagnosis.because();
    assert!(
        reasons.contains(".gitignore:1 'gen/'") && reasons.contains("hidden .cfg/"),
        "{reasons}"
    );
}

/// A walk that lists `.gitignore`d entries once the default prune is off
/// reaches the same file by either cause: the first, in cause order, is the
/// whole lead.
#[test]
fn causes_that_reach_the_same_files_are_alternatives() {
    let root = repo("gen/\n", &["gen"]);
    let reach = root.0.join("gen/needle.ts");
    let verdict = diagnose(&root.0, DEFAULTS, |walk| {
        // Lifting the ignore files or the default prune each finds it.
        let found = walk.ignored || !walk.default_excludes;
        Ok::<_, String>(Hits {
            count: usize::from(found),
            sample: if found {
                vec![reach.clone()]
            } else {
                Vec::new()
            },
            cut: false,
        })
    });
    let Ok(Verdict::Skipped(diagnosis)) = verdict else {
        panic!("{verdict:?}")
    };
    assert_eq!(diagnosis.causes(), [Cause::Ignored]);
}

#[test]
fn nothing_behind_the_skipped_paths_is_absent() {
    let root = repo("gen/\n", &[]);
    std::fs::create_dir_all(root.0.join("gen")).expect("gen");
    std::fs::write(root.0.join("gen/a.md"), "x\n").expect("file");
    let verdict = diagnose(&root.0, DEFAULTS, typescript(&root.0));
    assert!(matches!(verdict, Ok(Verdict::Absent)), "{verdict:?}");
}

#[test]
fn a_walk_that_skips_nothing_is_not_probed() {
    let calls = Cell::new(0);
    let walk = Walk {
        ignored: true,
        hidden: true,
        default_excludes: false,
    };
    let verdict = diagnose(Path::new("/nowhere"), walk, |_| {
        calls.set(calls.get() + 1);
        Ok::<_, String>(Hits::default())
    });
    assert!(matches!(verdict, Ok(Verdict::Absent)));
    assert_eq!(calls.get(), 0);
}

/// A probe cut before it found anything proves nothing: say so, do not
/// claim absence.
#[test]
fn a_cut_probe_that_found_nothing_is_unproven() {
    let verdict = diagnose(Path::new("/nowhere"), DEFAULTS, |_| {
        Ok::<_, String>(Hits {
            cut: true,
            ..Hits::default()
        })
    });
    assert!(matches!(verdict, Ok(Verdict::Unproven)), "{verdict:?}");
}

/// The cheap exit and the minimal search cost few walks: one when a single
/// cause is off, one when nothing is found, and a single cause revealing
/// stops at the singles.
#[test]
fn probes_are_few() {
    let found = |walk: Walk| {
        // Only a hidden file exists.
        Ok::<_, String>(if walk.hidden {
            Hits {
                count: 1,
                sample: vec![PathBuf::from("/r/.cfg/a.ts")],
                cut: false,
            }
        } else {
            Hits::default()
        })
    };
    for (walk, calls) in [
        (
            Walk {
                ignored: true,
                hidden: false,
                default_excludes: false,
            },
            1,
        ),
        (DEFAULTS, 4),
    ] {
        let count = Cell::new(0);
        let verdict = diagnose(Path::new("/r"), walk, |walk| {
            count.set(count.get() + 1);
            found(walk)
        });
        match verdict {
            Ok(Verdict::Skipped(diagnosis)) => assert_eq!(diagnosis.causes(), [Cause::Hidden]),
            other => panic!("{other:?}"),
        }
        assert_eq!(count.get(), calls, "{walk:?}");
    }
}

/// More reasons than a hint names collapse to a count, not silence.
#[test]
fn reasons_beyond_the_named_few_are_counted() {
    let dirs = [".a", ".b", ".c", ".d", ".e"];
    let root = repo("", &dirs);
    let diagnosis = skipped(&root.0, DEFAULTS);
    assert_eq!(diagnosis.skips().len(), 5);
    assert!(
        diagnosis.because().ends_with(", +2 more"),
        "{}",
        diagnosis.because()
    );
}

/// A rule above the search root reads root-relative; a `!` re-include ends
/// the search.
#[test]
fn ignore_rules_read_root_relative_and_honor_reincludes() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "*.log\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("sub")).expect("sub");
    let sub = root.0.join("sub");
    assert_eq!(
        ignore_rule(&sub, &sub.join("x.log"), false).as_deref(),
        Some("../.gitignore:1 '*.log'")
    );
    std::fs::write(sub.join(".gitignore"), "!x.log\n").expect("nested");
    assert_eq!(ignore_rule(&sub, &sub.join("x.log"), false), None);
}
