use super::*;

use crate::security::scan::Passthrough;

fn test_context() -> RequestContext {
    RequestContext::with_timeout(std::time::Duration::from_secs(30), MAX_SCAN_BYTES)
}

/// A hit on a name the grammar's outline declares carries its
/// declaration line, so the read lead returns the whole block; a call,
/// a comment or a repeated line does not.
#[test]
fn a_declared_hit_names_its_unique_declaration_line() {
    let declared_in = |path: &str, text: &str, keyword: &str| match scan(
        text.as_bytes(),
        path,
        &[keyword.into()],
        "",
        &Passthrough,
    ) {
        FileHits::Lines { declaration, .. } => declaration,
        other => panic!("{other:?}"),
    };
    let declaration = |text: &str, keyword: &str| declared_in("a.py", text, keyword);
    let text =
        "x = 1\n\ndef merge_setting(request, session):\n    return 1\n\nmerge_setting(a, b)\n";
    assert_eq!(
        declaration(text, "merge_setting"),
        Some("def merge_setting(request, session):".to_owned())
    );
    assert_eq!(declaration("merge_setting(a, b)\n", "merge_setting"), None);
    // The searched keyword may carry the block keyword itself.
    assert_eq!(
        declaration(text, "def merge_setting"),
        Some("def merge_setting(request, session):".to_owned())
    );
    assert_eq!(
        declaration(
            "def merge_setting(a):\n    pass\ndef merge_setting(a):\n    pass\n",
            "merge_setting"
        ),
        None
    );
    assert_eq!(
        declared_in(
            "server.go",
            "package http\n\nfunc (s *Server) ListenAndServe() error {\n\treturn nil\n}\n",
            "listenandserve"
        ),
        Some("func (s *Server) ListenAndServe() error {".to_owned())
    );
    // A parameter named like the keyword is no declaration of it.
    assert_eq!(
        declaration("def run(merge_setting):\n    pass\n", "merge_setting"),
        None
    );
    assert_eq!(
        declaration("# def merge_setting(a):\n", "merge_setting"),
        None
    );
}

/// The resume line is where the next unshown hit can start, only
/// when the shown lines are exactly the first hits and none was cut.
#[test]
fn a_listing_that_is_a_hit_prefix_resumes_after_its_last_line() {
    let text = "x\n".repeat(2) + &"hit()\n".repeat(8);
    let FileHits::Lines {
        lines,
        total,
        resume_after,
        ..
    } = scan(text.as_bytes(), "a.rs", &["hit".into()], "", &Passthrough)
    else {
        panic!("hits")
    };
    assert_eq!((lines.len(), total), (MAX_LINES_PER_FILE, 8));
    assert_eq!(resume_after, Some(7));
    // The last hit is shown: nothing is left to continue.
    let text = "hit()\n".repeat(3);
    let FileHits::Lines { resume_after, .. } =
        scan(text.as_bytes(), "a.rs", &["hit".into()], "", &Passthrough)
    else {
        panic!("hits")
    };
    assert_eq!(resume_after, Some(3));
    // A listing that skipped a hit line (a keyword-only line beside
    // lines holding every keyword) is not a prefix of the hits.
    let text = "alpha\n".to_owned() + &"alpha beta\n".repeat(6);
    let FileHits::Lines { resume_after, .. } = scan(
        text.as_bytes(),
        "a.rs",
        &["alpha".into(), "beta".into()],
        "",
        &Passthrough,
    ) else {
        panic!("hits")
    };
    assert_eq!(resume_after, None);
    // A cut line cannot be resumed after: its tail was not shown.
    let text = format!("hit {}\n{}", "a".repeat(400), "hit\n".repeat(7));
    let FileHits::Lines { resume_after, .. } =
        scan(text.as_bytes(), "a.rs", &["hit".into()], "", &Passthrough)
    else {
        panic!("hits")
    };
    assert_eq!(resume_after, None);
}

#[test]
fn every_keyword_line_is_numbered_case_insensitively() {
    let text = "import x\nawait Wrap_App(app)\nother\n    await wrap_app(s)\n";
    let hits = scan(
        text.as_bytes(),
        "a.py",
        &["wrap_app".into()],
        "",
        &Passthrough,
    );
    assert_eq!(
        hits,
        FileHits::Lines {
            lines: vec![
                "2\tawait Wrap_App(app)".into(),
                "4\t    await wrap_app(s)".into()
            ],
            first: 2,
            last: 4,
            best: 2,
            total: 2,
            line_count: 4,
            clipped: false,
            declaration: None,
            resume_after: Some(4),
        }
    );
    assert_eq!(
        scan(
            text.as_bytes(),
            "a.py",
            &["absent".into()],
            "",
            &Passthrough
        ),
        FileHits::Unmatched
    );
    assert_eq!(
        scan(
            b"bin\0ary needle",
            "a.bin",
            &["needle".into()],
            "",
            &Passthrough
        ),
        FileHits::Unavailable
    );
}

/// The read anchors on the hit that best fits the goal: a literal
/// assigned to the keyword beats an earlier type declaration, a later
/// use, and a setter whose docs name more goal terms; goal terms nearby
/// break ties between hits of the same shape.
#[test]
fn the_best_hit_for_the_goal_anchors_the_read() {
    let mut text = String::from(
        "struct Builder {\n    /// Cap on thread usage.\n    max_blocking_threads: usize,\n}\n",
    );
    text.push_str(&"\n".repeat(20));
    text.push_str("fn new() -> Builder {\n    Builder {\n        // Defaults for the pool.\n        max_blocking_threads: 512,\n    }\n}\n");
    text.push_str(&"\n".repeat(20));
    text.push_str("fn spawn(&self) {\n    pool(self.max_blocking_threads);\n}\n");
    // A setter whose docs name more goal terms is still a declaration,
    // not the default value.
    text.push_str("/// The default value is set by the Builder runtime.\npub fn max_blocking_threads(&mut self, val: usize) {\n    self.max_blocking_threads = val;\n}\n");
    let best = |goal: &str| match scan(
        text.as_bytes(),
        "builder.rs",
        &["max_blocking_threads".into()],
        goal,
        &Passthrough,
    ) {
        FileHits::Lines { first, best, .. } => (first, best),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        best("Find default max_blocking_threads in the Builder"),
        (3, 28)
    );
    // Without goal terms a literal assignment still outranks a type.
    assert_eq!(best(""), (3, 28));
    // A comment hit never anchors over code.
    let commented =
        "// max_blocking_threads = 1 by default\nlet max_blocking_threads = config();\n";
    let FileHits::Lines { best, .. } = scan(
        commented.as_bytes(),
        "a.rs",
        &["max_blocking_threads".into()],
        "default max_blocking_threads",
        &Passthrough,
    ) else {
        panic!("hit");
    };
    assert_eq!(best, 2);
}

#[test]
fn long_lines_keep_a_window_around_the_keyword() {
    let line = format!("{}needle{}", "a".repeat(500), "b".repeat(500));
    let FileHits::Lines { lines, clipped, .. } = scan(
        line.as_bytes(),
        "a.js",
        &["needle".into()],
        "",
        &Passthrough,
    ) else {
        panic!("hit");
    };
    // The cut is flagged so the page carries a read of the whole line.
    assert!(clipped);
    assert!(lines[0].starts_with("1\t…"), "{}", lines[0]);
    assert!(lines[0].contains("needle"));
    assert!(lines[0].chars().count() <= MAX_LINE_CHARS + 4);
}

/// Several keywords list the lines holding every keyword; lines with
/// only some stay counted for the readHits continuation. A file where no
/// line holds them all lists each keyword's first line.
#[test]
fn lines_with_every_keyword_are_listed_first() {
    let keywords = ["octocode".to_owned(), "mcp".to_owned()];
    let text = "# Octocode\nOctocode MCP server\nmcp only\noctocode only\nthe octocode mcp tools\n";
    let FileHits::Lines { lines, total, .. } =
        scan(text.as_bytes(), "README.md", &keywords, "", &Passthrough)
    else {
        panic!("hit");
    };
    assert_eq!(
        lines,
        vec!["2\tOctocode MCP server", "5\tthe octocode mcp tools"]
    );
    assert_eq!(total, 5);
    let spread = "octocode one\noctocode two\nmcp three\nmcp four\n";
    let FileHits::Lines { lines, total, .. } =
        scan(spread.as_bytes(), "a.md", &keywords, "", &Passthrough)
    else {
        panic!("hit");
    };
    // The window anchoring the read (lines 2-3) is listed too.
    assert_eq!(
        lines,
        vec!["1\toctocode one", "2\toctocode two", "3\tmcp three"]
    );
    assert_eq!(total, 4);
}

#[test]
fn the_read_anchors_on_a_line_holding_every_keyword() {
    let keywords = ["fn".to_owned(), "search_path".to_owned()];
    let text = "fn default() {}\nfn other() {}\nlet x = 1;\nfn search_path(p: &Path) {}\n";
    let FileHits::Lines {
        first, last, best, ..
    } = scan(text.as_bytes(), "a.rs", &keywords, "", &Passthrough)
    else {
        panic!("hit");
    };
    assert_eq!((first, last, best), (4, 4, 4));
}

/// With no line holding every keyword, the read anchors on the
/// smallest window that holds them all, preferring one a declaration
/// opens, not on the best single-keyword hit elsewhere in the file.
#[test]
fn spread_keywords_anchor_on_the_declaration_window() {
    let mut lines = vec![String::new(); 900];
    lines[0] = "class Session:".into();
    lines[127] = "    def configure(self, env):".into();
    lines[128] = "        self.trust_env = env".into();
    lines[490] = "    def reset(self):".into();
    lines[491] = "        self.trust_env = True".into();
    lines[639] = "    def request(self, url):".into();
    lines[640] = "        settings = self.merge_environment_settings(url)".into();
    lines[830] =
        "    def merge_environment_settings(self, url, proxies, stream, verify, cert):".into();
    lines[844] = "        if self.trust_env:".into();
    let text = lines.join("\n");
    let keywords = [
        "merge_environment_settings".to_owned(),
        "trust_env".to_owned(),
    ];
    let FileHits::Lines {
        lines,
        first,
        last,
        best,
        declaration,
        ..
    } = scan(text.as_bytes(), "sessions.py", &keywords, "", &Passthrough)
    else {
        panic!("hit");
    };
    assert_eq!((first, last, best), (831, 845, 831));
    assert_eq!(
        declaration.as_deref(),
        Some("def merge_environment_settings(self, url, proxies, stream, verify, cert):")
    );
    let numbers: Vec<&str> = lines
        .iter()
        .filter_map(|line| line.split('\t').next())
        .collect();
    for number in ["129", "641", "831", "845"] {
        assert!(numbers.contains(&number), "{lines:?}");
    }
    // Keywords never within the window size keep the best single hit.
    let far = "x = trust_env\n".to_owned() + &"\n".repeat(100) + "merge_environment_settings()\n";
    let FileHits::Lines { best, .. } = scan(far.as_bytes(), "a.py", &keywords, "", &Passthrough)
    else {
        panic!("hit");
    };
    assert_eq!(best, 1);
}

/// A hit line whose lowercase form moves char boundaries, even at an
/// equal byte length, lists verbatim rather than panicking, and a long
/// line keeps its keyword in the window.
#[test]
fn hit_lines_that_lowercase_to_other_boundaries_list_verbatim() {
    let lines = |text: &str, keyword: &str| match scan(
        text.as_bytes(),
        "a.js",
        &[keyword.into()],
        "",
        &Passthrough,
    ) {
        FileHits::Lines { lines, .. } => lines,
        other => panic!("{other:?}"),
    };
    let text = "const s = \"İéxẞ\";\n";
    assert_eq!(text.len(), text.to_lowercase().len());
    assert_eq!(lines(text, "éx"), ["1\tconst s = \"İéxẞ\";"]);
    assert_eq!(lines(text, "ẞ"), ["1\tconst s = \"İéxẞ\";"]);
    assert_eq!(lines("x = \"STRAẞE\"\n", "straße"), ["1\tx = \"STRAẞE\""]);
    for prefix in ["İ", "ẞ", "e\u{301}", "👍🏽"] {
        let line = format!("{}needle{}", prefix.repeat(300), "b".repeat(300));
        let shown = &lines(&line, "needle")[0];
        assert!(shown.starts_with("1\t…"), "{prefix}: {shown}");
        assert!(shown.contains("needle"), "{prefix}: {shown}");
    }
}

/// Scanning a page's files on parallel threads keeps each file's own
/// hits, in page order.
#[test]
fn parallel_scans_keep_page_order() {
    let texts: Vec<String> = (0..17)
        .map(|file| match file % 3 {
            0 => format!("fn f{file}() {{}}\nneedle {file}\n"),
            1 => "nothing here\n".to_owned(),
            _ => format!("{}needle\n", "x\n".repeat(file)),
        })
        .collect();
    let paths: Vec<String> = (0..texts.len()).map(|file| format!("f{file}.rs")).collect();
    let files: Vec<(&[u8], &str)> = texts
        .iter()
        .zip(&paths)
        .map(|(text, path)| (text.as_bytes(), path.as_str()))
        .collect();
    let keywords = ["needle".to_owned()];
    let serial: Vec<FileHits> = files
        .iter()
        .map(|&(bytes, path)| scan(bytes, path, &keywords, "", &Passthrough))
        .collect();
    assert_eq!(
        scan_all(&files, &keywords, "", &Passthrough, &test_context()).unwrap(),
        serial
    );
    assert!(matches!(serial[1], FileHits::Unmatched));
    assert!(matches!(serial[5], FileHits::Lines { first: 6, .. }));
}

/// A file with many declarations finds the one a hit names by its line:
/// declarations on other lines never match, and outline order holds on
/// one line.
#[test]
fn hits_find_their_declaration_by_line_among_many() {
    let mut text = String::new();
    for function in 0..2_000 {
        text.push_str(&format!("def f{function}(a):\n    return a\n"));
    }
    text.push_str("def target_name(a):\n    return a\n");
    let FileHits::Lines {
        declaration, best, ..
    } = scan(
        text.as_bytes(),
        "a.py",
        &["target_name".into()],
        "",
        &Passthrough,
    )
    else {
        panic!("hit");
    };
    assert_eq!(best, 4_001);
    assert_eq!(declaration.as_deref(), Some("def target_name(a):"));
}

/// A scanned file keeps only the declarations named on its keyword
/// lines, borrowed from the shared parse: a file of 2,000 functions
/// with one hit holds one.
#[test]
fn only_declarations_named_on_hit_lines_are_kept() {
    let mut text = String::new();
    for function in 0..2_000 {
        text.push_str(&format!("def f{function}(a):\n    return a\n"));
    }
    text.push_str("def target_name(a):\n    return a\n");
    let path = format!("hit-declarations-{}.py", std::process::id());
    let outline = crate::tools::declarations_cache::file_document(&text, &path).expect("outline");
    assert_eq!(outline.declarations.len(), 2_001);
    let kept = hit_declarations(Some(&outline), &[(4_000, 4, 11)]);
    let names: Vec<&str> = kept.iter().map(|declared| declared.name.as_str()).collect();
    assert_eq!(names, ["target_name"]);
    assert!(hit_declarations(None, &[(4_000, 4, 11)]).is_empty());
}

/// Fan-outs running at once (rows, or calls of a long-lived server)
/// share one byte budget: the bytes held by running items never pass it,
/// an item larger than the whole budget runs alone, and each fan-out
/// keeps its items' order.
#[test]
fn concurrent_fan_outs_hold_their_bytes_within_one_budget() {
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    let budget = ScanBudget::new(10);
    let (held, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let mut items: Vec<usize> = (0..24).map(|item| 1 + item % 4).collect();
    items.push(25);
    let work = |&weight: &usize| {
        let charged = weight.min(10);
        peak.fetch_max(held.fetch_add(charged, SeqCst) + charged, SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(2));
        held.fetch_sub(charged, SeqCst);
        weight * 10
    };
    let outs = std::thread::scope(|scope| {
        let calls: Vec<_> = (0..3)
            .map(|_| {
                scope.spawn(|| fan_out(&budget, &test_context(), 8, &items, |&weight| weight, work))
            })
            .collect();
        calls
            .into_iter()
            .map(|call| call.join().expect("fan-out").expect("scan"))
            .collect::<Vec<_>>()
    });
    assert!(peak.load(SeqCst) <= 10, "{}", peak.load(SeqCst));
    let expected: Vec<Option<usize>> = items.iter().map(|weight| Some(weight * 10)).collect();
    for out in outs {
        assert_eq!(out, expected);
    }
    assert_eq!(*budget.held.lock().expect("held"), 0);
}

/// Workers follow the cores for small blobs and the budget for large
/// ones: twelve 1.8 MB blobs scan four at a time on twelve cores.
#[test]
fn scan_workers_are_bounded_by_bytes_then_cores() {
    let budget = ScanBudget::new(SCAN_BUDGET_BYTES);
    assert_eq!(budget.workers(12, 12, 1_800_000), 4);
    assert_eq!(budget.workers(12, 12, 20_000), 12);
    assert_eq!(budget.workers(12, 3, 20_000), 3);
    assert_eq!(budget.workers(12, 12, MAX_SCAN_BYTES), 4);
    assert_eq!(budget.workers(1, 12, 0), 1);
    assert_eq!(scan_weight(&vec![b'x'; MAX_SCAN_BYTES + 1]), 0);
}

/// A fan-out given one core runs on one thread, with the serial output.
#[test]
fn a_fan_out_on_one_core_runs_on_one_thread() {
    let threads = std::sync::Mutex::new(std::collections::HashSet::new());
    let out = fan_out(
        &ScanBudget::new(usize::MAX),
        &test_context(),
        1,
        &[1, 2, 3, 4],
        |_| 0,
        |&item| {
            threads
                .lock()
                .expect("threads")
                .insert(std::thread::current().id());
            item
        },
    );
    assert_eq!(out.unwrap(), [Some(1), Some(2), Some(3), Some(4)]);
    assert_eq!(threads.lock().expect("threads").len(), 1);
}

/// A panicking item is the only one lost, on a worker that took others
/// too and on the single-item path alike, and its bytes go back.
#[test]
fn a_panicking_item_loses_only_itself() {
    let budget = ScanBudget::new(usize::MAX);
    let work = |&item: &u32| {
        assert_ne!(item, 2, "item 2 panics");
        item
    };
    let out = fan_out(&budget, &test_context(), 1, &[1, 2, 3, 4], |_| 1, work);
    assert_eq!(out.unwrap(), [Some(1), None, Some(3), Some(4)]);
    assert_eq!(
        fan_out(&budget, &test_context(), 1, &[2], |_| 1, work).unwrap(),
        [None]
    );
    assert_eq!(*budget.held.lock().expect("held"), 0);
}

fn stopped_budget_waiter(timeout: bool, items: &[usize]) {
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::time::{Duration, Instant};

    let budget = ScanBudget::new(1);
    let holder_context = test_context();
    let holder = budget.admit(1, &holder_context).expect("holder admitted");
    let mut context = test_context();
    if timeout {
        context.budget.deadline = Instant::now() + Duration::from_millis(250);
        context.budget.wait_until = context.budget.deadline;
    }
    let ran = AtomicUsize::new(0);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let result = fan_out(
                &budget,
                &context,
                4,
                items,
                |_| 1,
                |_| {
                    ran.fetch_add(1, SeqCst);
                },
            );
            tx.send(result.map_err(|error| error.kind))
                .expect("result receiver");
        });
        let observe_until = Instant::now() + Duration::from_secs(2);
        let waiting = loop {
            if budget.waiting.load(SeqCst) != 0 {
                break true;
            }
            if Instant::now() >= observe_until {
                break false;
            }
            std::thread::yield_now();
        };
        if !timeout {
            context.budget.cancellation.cancel();
        }
        let result = rx.recv_timeout(Duration::from_secs(2));
        let still_held = *budget.held.lock().expect("budget");
        let work_ran = ran.load(SeqCst);
        // Release even on failure so a broken unbounded wait cannot hang the test.
        drop(holder);
        assert!(waiting, "worker must enter admission wait before stopping");
        assert_eq!(
            result.expect("waiter returns before holder release"),
            Err(if timeout {
                ProviderErrorKind::Timeout
            } else {
                ProviderErrorKind::Cancelled
            })
        );
        assert_eq!(still_held, 1);
        assert_eq!(work_ran, 0);
    });
    assert_eq!(*budget.held.lock().expect("budget"), 0);
}

#[test]
fn cancelled_budget_waiter_returns_before_holder_release_without_work() {
    stopped_budget_waiter(false, &[1]);
    stopped_budget_waiter(false, &[1, 2]);
}

#[test]
fn expired_budget_waiter_returns_before_holder_release_without_work() {
    stopped_budget_waiter(true, &[1]);
    stopped_budget_waiter(true, &[1, 2]);
}

#[test]
fn cancellation_between_files_stops_remaining_work() {
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    let budget = ScanBudget::new(1);
    let context = test_context();
    let ran = AtomicUsize::new(0);
    let result = fan_out(
        &budget,
        &context,
        1,
        &[1, 2, 3],
        |_| 1,
        |_| {
            ran.fetch_add(1, SeqCst);
            context.budget.cancellation.cancel();
        },
    );
    assert_eq!(result.unwrap_err().kind, ProviderErrorKind::Cancelled);
    assert_eq!(ran.load(SeqCst), 1);
    assert_eq!(*budget.held.lock().expect("budget"), 0);
}

#[test]
fn deadline_between_files_stops_remaining_work() {
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::time::{Duration, Instant};
    let budget = ScanBudget::new(1);
    let mut context = test_context();
    context.budget.deadline = Instant::now() + Duration::from_millis(100);
    let ran = AtomicUsize::new(0);
    let result = fan_out(
        &budget,
        &context,
        1,
        &[1, 2, 3],
        |_| 1,
        |_| {
            ran.fetch_add(1, SeqCst);
            while Instant::now() < context.budget.deadline {
                std::thread::park_timeout(
                    context
                        .budget
                        .deadline
                        .saturating_duration_since(Instant::now()),
                );
            }
        },
    );
    assert_eq!(result.unwrap_err().kind, ProviderErrorKind::Timeout);
    assert_eq!(ran.load(SeqCst), 1);
    assert_eq!(*budget.held.lock().expect("budget"), 0);
}

#[test]
fn stopped_scan_all_propagates_errors_instead_of_unavailable_hits() {
    let context = test_context();
    context.budget.cancellation.cancel();
    let result = scan_all(
        &[(b"needle", "a.rs")],
        &["needle".into()],
        "",
        &Passthrough,
        &context,
    );
    assert_eq!(result.unwrap_err().kind, ProviderErrorKind::Cancelled);
    let mut context = test_context();
    context.budget.deadline = std::time::Instant::now();
    let result = scan_all(&[], &[], "", &Passthrough, &context);
    assert_eq!(result.unwrap_err().kind, ProviderErrorKind::Timeout);
}

#[test]
fn many_hits_are_capped_but_counted() {
    let text = "needle\n".repeat(30);
    let FileHits::Lines {
        lines,
        total,
        last,
        clipped,
        ..
    } = scan(
        text.as_bytes(),
        "a.rs",
        &["needle".into()],
        "",
        &Passthrough,
    )
    else {
        panic!("hit");
    };
    assert_eq!(lines.len(), MAX_LINES_PER_FILE);
    assert_eq!(total, 30);
    assert_eq!(last, 30);
    assert!(!clipped);
}
