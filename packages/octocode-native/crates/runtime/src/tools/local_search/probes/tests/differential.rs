//! A walk that reports nothing skipped must leave nothing for a relaxed walk
//! to find: the proof that the empty page's check can be left out then. Every
//! thing a walk can leave out, alone and combined, under every request shape
//! that changes what the walk records.

use super::*;

/// One thing a tree can hold. Paths are under the repository; the search root
/// is its `sub/`. Each file with `needle` is a miss the search must not reach,
/// `rules` joins the repository's `.gitignore`.
struct Atom {
    files: &'static [&'static str],
    rules: &'static str,
    /// `(link, target)`, the target under the repository.
    links: &'static [(&'static str, &'static str)],
}

const fn files(files: &'static [&'static str]) -> Atom {
    Atom {
        files,
        rules: "",
        links: &[],
    }
}

const fn ignored(files: &'static [&'static str], rules: &'static str) -> Atom {
    Atom {
        files,
        rules,
        links: &[],
    }
}

const fn links(links: &'static [(&'static str, &'static str)], rules: &'static str) -> Atom {
    Atom {
        files: &[],
        rules,
        links,
    }
}

const ATOMS: &[Atom] = &[
    // Ignore rules: above the root, inside it, the git exclude file.
    ignored(&["sub/gen/n.txt"], "gen/\n"),
    ignored(&["sub/n.log"], "*.log\n"),
    files(&["sub/deep/.gitignore:n.txt", "sub/deep/n.txt"]),
    files(&["sub/deep2/.ignore:n.txt", "sub/deep2/n.txt"]),
    files(&[".git/info/exclude:excl.txt", "sub/excl.txt"]),
    // A nested repository's own rules.
    files(&[
        "sub/inner/.git/HEAD",
        "sub/inner/.gitignore:n.txt",
        "sub/inner/n.txt",
    ]),
    // Dot entries.
    files(&["sub/.dot.txt"]),
    files(&["sub/.cfg/n.txt"]),
    files(&["sub/ok/.nested/deep/n.txt"]),
    ignored(&["sub/.ignored-dot/n.txt"], ".ignored-dot/\n"),
    // Default-pruned directories: by name, under a dot, behind a rule.
    files(&["sub/node_modules/p/n.txt"]),
    files(&["sub/.cache/n.txt"]),
    ignored(&["sub/target/n.txt"], "target/\n"),
    files(&["sub/ok/build/n.txt"]),
    // Generated files.
    files(&["sub/n.min.js"]),
    files(&["sub/n.js.map"]),
    files(&["sub/n.tmp"]),
    files(&["sub/.DS_Store"]),
    // Links, which no walk follows: plain, hidden, ignored, dangling, to a
    // directory.
    links(&[("sub/l.txt", "outside/n.txt")], ""),
    links(&[("sub/.l.txt", "outside/n.txt")], ""),
    links(&[("sub/il.txt", "outside/n.txt")], "il.txt\n"),
    links(&[("sub/dangling.txt", "gone/n.txt")], ""),
    links(&[("sub/ldir", "outside")], ""),
    links(&[("sub/.ldir", "outside")], ""),
    // What the path policy withholds whatever the flags.
    files(&["sub/.env", "sub/.env.local", "sub/id_rsa", "sub/a.pem"]),
    files(&["sub/.ssh/n.txt", "sub/.aws/n.txt", "sub/.git/n.txt"]),
    ignored(&["sub/x.key"], "*.key\n"),
    // A directory named like a credential file is source.
    files(&["sub/.env.d/n.txt", "sub/.secrets/n.txt"]),
    ignored(&["sub/auth.json/n.txt"], "auth.json/\n"),
];

/// Request shapes: each a term and fields. The miss is `needle`; a files-without
/// row looks for what the fixtures' plain file holds.
fn shapes() -> Vec<Value> {
    let needle = |fields: Value| {
        let mut row = json!({"matchString": ["needle"]});
        row.as_object_mut()
            .expect("row")
            .extend(fields.as_object().expect("fields").clone());
        row
    };
    vec![
        needle(json!({})),
        needle(json!({"include": ["*.txt"]})),
        needle(json!({"include": ["*.js"]})),
        needle(json!({"include": ["plain/**"]})),
        needle(json!({"include": ["gen/**"]})),
        needle(json!({"include": [".cfg/**"]})),
        needle(json!({"include": ["node_modules/**"]})),
        needle(json!({"include": ["**/n.txt"]})),
        needle(json!({"exclude": ["*.txt"]})),
        needle(json!({"exclude": [".cfg"]})),
        needle(json!({"fileType": "txt"})),
        needle(json!({"maxDepth": 1})),
        needle(json!({"maxDepth": 2})),
        needle(json!({"includeHidden": true})),
        needle(json!({"includeIgnored": true})),
        needle(json!({"defaultExcludes": false})),
        needle(json!({"view": "files"})),
        needle(json!({"caseMode": "sensitive"})),
        json!({"matchString": ["plain text"], "view": "filesWithout"}),
    ]
}

fn build(atoms: &[&Atom], repository: bool) -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, root) = tree();
    write(&root, "sub/plain/p.txt", "plain text\n");
    write(&root, "outside/n.txt", "needle\n");
    if repository {
        fs::create_dir_all(root.join(".git")).expect("marker");
    }
    let mut rules = String::new();
    for atom in atoms {
        rules.push_str(atom.rules);
        for file in atom.files {
            // `path:text` writes a rule file; a bare path holds the needle.
            match file.split_once(':') {
                Some((path, text)) => write(&root, path, &format!("{text}\n")),
                None => write(&root, file, "needle\n"),
            }
        }
        for (link, target) in atom.links {
            let at = root.join(link);
            fs::create_dir_all(at.parent().expect("parent")).expect("directory");
            #[cfg(unix)]
            std::os::unix::fs::symlink(root.join(target), &at).expect("link");
        }
    }
    if repository {
        write(&root, ".gitignore", &rules);
    }
    (dir, root)
}

/// What the differential has seen: a tally proves it ran both ways.
#[derive(Default)]
struct Tally {
    nothing_named: usize,
    named_and_found: usize,
}

fn check(root: &Path, shape: &Value, label: &str, tally: &mut Tally) {
    let paths = crate::tools::test_support::workspace_policy(root);
    let row = query(&root.join("sub"), shape.clone());
    let first = scan_query(&row, &paths, &NeverCancel, (None, None)).expect("first walk");
    let names = |files: &[octocode_engine::types::TextSearchFile]| {
        files
            .iter()
            .map(|file| file.path.clone())
            .collect::<BTreeSet<_>>()
    };
    let walked = names(&first.parsed.files);
    let options = &first.search_options;
    let mut lifted = options.probe(if options.output == SearchOutput::FilesWithoutMatch {
        SearchOutput::FilesWithoutMatch
    } else {
        SearchOutput::FilesWithMatch
    });
    lifted.ignore = octocode_engine::types::IgnorePolicy {
        include_ignored: true,
        include_hidden: true,
    };
    let (relaxed, cut, _) = with_probe_budget(Duration::from_secs(600), || {
        probe_walked(lifted, &paths, false, &NeverCancel)
    })
    .expect("relaxed walk");
    assert!(!cut, "{label}");
    let new = names(&relaxed.files)
        .difference(&walked)
        .cloned()
        .collect::<Vec<_>>();
    if names_nothing(&first.skipped) {
        tally.nothing_named += 1;
        assert!(
            new.is_empty(),
            "{label}: the first walk named nothing but the relaxed walk found {new:?}; skips: {:?}",
            first.skipped
        );
    } else if !new.is_empty() {
        tally.named_and_found += 1;
    }
}

#[test]
fn a_walk_that_names_nothing_leaves_nothing_for_a_relaxed_walk() {
    let shapes = shapes();
    let mut tally = Tally::default();
    for repository in [true, false] {
        // Each thing alone, then everything, then mixed subsets from a fixed
        // sequence.
        let mut subsets = (0..ATOMS.len())
            .map(|index| vec![&ATOMS[index]])
            .collect::<Vec<_>>();
        subsets.push(vec![]);
        subsets.push(ATOMS.iter().collect());
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        for _ in 0..12 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            subsets.push(
                ATOMS
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| state >> (index % 61) & 3 == 0)
                    .map(|(_, atom)| atom)
                    .collect(),
            );
        }
        for (index, subset) in subsets.iter().enumerate() {
            let (_dir, root) = build(subset, repository);
            for shape in &shapes {
                let label = format!("repository {repository}, subset {index}, {shape}");
                check(&root, shape, &label, &mut tally);
            }
        }
    }
    assert!(tally.nothing_named > 50, "{}", tally.nothing_named);
    assert!(tally.named_and_found > 50, "{}", tally.named_and_found);
}
