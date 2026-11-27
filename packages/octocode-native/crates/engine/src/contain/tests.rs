use super::*;

#[test]
fn a_panic_is_contained_with_its_phase_and_site() {
    let contained =
        contain(Phase::Extract, || -> u8 { panic!("quoted input") }).expect_err("panicked");
    assert_eq!(contained.phase, Phase::Extract);
    // The site is where the work panicked: this file, then line and column.
    let site = contained.site.as_deref().unwrap_or_default();
    let (file, line_column) = site.split_once(':').unwrap_or_default();
    assert_eq!(file, portable(file!()), "{site}");
    let numbers = line_column
        .split(':')
        .map(str::parse::<u32>)
        .collect::<Vec<_>>();
    assert!(matches!(numbers.as_slice(), [Ok(_), Ok(_)]), "{site}");
    assert!(!contained.to_string().contains("quoted input"));
}

#[test]
fn a_recovered_inner_panic_still_marks_the_work_unassessed() {
    let contained = contain(Phase::Extract, || {
        within(Phase::Parse, || {
            std::panic::catch_unwind(|| -> u8 { panic!("inner") }).unwrap_or(0)
        })
    })
    .expect_err("an inner catch recovered");
    assert_eq!(contained.phase, Phase::Parse);
}

#[test]
fn catch_keeps_a_result_an_inner_catch_recovered() {
    let value = catch(Phase::Extract, || {
        std::panic::catch_unwind(|| -> u8 { panic!("inner") }).unwrap_or(7)
    });
    assert_eq!(value, Ok(7));
    assert!(catch(Phase::Extract, || -> u8 { panic!("outer") }).is_err());
}

#[test]
fn work_on_another_thread_reports_to_the_scope_that_handed_it_over() {
    let contained = contain(Phase::SecretScan, || {
        let scope = current();
        std::thread::spawn(move || {
            adopt(scope, || std::panic::catch_unwind(|| -> u8 { panic!("x") }))
        })
        .join()
        .map(|_| ())
    })
    .expect_err("the worker panicked");
    assert_eq!(contained.phase, Phase::SecretScan);
}

#[test]
fn a_site_keeps_no_build_machine_directory() {
    let registry = "/home/u/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/oxc_parser-0.30.0/src/lexer/mod.rs";
    assert_eq!(portable(registry), "oxc_parser-0.30.0/src/lexer/mod.rs");
    assert_eq!(
        portable("/rustc/0a1b2c/library/core/src/option.rs"),
        "core/src/option.rs"
    );
    assert_eq!(portable("/tmp/build/out/generated.rs"), "generated.rs");
    assert_eq!(
        portable("crates/engine/src/contain.rs"),
        "crates/engine/src/contain.rs"
    );
}

/// Contained panics, one on a thread that adopted the scope; run only as
/// the child of [`a_contained_panic_prints_nothing`].
#[test]
#[ignore = "the child process of a_contained_panic_prints_nothing"]
fn contained_panics_in_a_child() {
    let caught = contain(Phase::Extract, || -> u8 { panic!("quoted input") });
    let recovered = catch(Phase::Parse, || {
        let scope = current();
        std::thread::spawn(move || {
            adopt(scope, || {
                std::panic::catch_unwind(|| -> u8 { panic!("inner") })
            })
        })
        .join()
        .map(|_| ())
    });
    assert!(caught.is_err() && recovered.is_ok());
}

/// A contained panic prints nothing: run in a child process with the
/// test harness's capture off, so the hook's output would reach stderr.
#[test]
fn a_contained_panic_prints_nothing() {
    let exe = std::env::current_exe().expect("test binary");
    let out = std::process::Command::new(exe)
        .args([
            "--exact",
            "contain::tests::contained_panics_in_a_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .expect("run the child");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("1 passed"),
        "the child ran the test"
    );
    assert!(
        !stderr.contains("panicked") && !stderr.contains("quoted input"),
        "{stderr}"
    );
}

#[test]
fn a_panic_under_no_scope_reaches_the_replaced_hook() {
    assert!(!record(None));
    assert!(contain(Phase::Link, || 1).is_ok());
    assert!(current().is_none());
}
