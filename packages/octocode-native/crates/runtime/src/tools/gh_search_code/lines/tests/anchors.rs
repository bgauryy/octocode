//! Where a scanned file's read anchors: the best hit, the covering window and declarations.
use super::*;

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
