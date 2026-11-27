use super::*;

#[test]
fn interned_strings_that_share_a_hash_keep_their_own_ids() {
    let mut interner = Interner::default();
    let (first, second) = (interner.id_hashed(7, "a"), interner.id_hashed(7, "b"));
    assert_eq!((first, second), (0, 1));
    assert_eq!(interner.id_hashed(7, "a"), first);
    assert_eq!(interner.id_hashed(7, "b"), second);
    assert_eq!(interner.id_hashed(9, "c"), 2);
    assert_eq!(interner.id("d"), 3);
    assert_eq!(interner.id("d"), 3);
    assert_eq!(interner.strings, ["a", "b", "c", "d"]);
}

/// The text the tables store a value as is what a `serde_json::Value` of it
/// prints, field for field, whatever the value holds (tagged enums, skipped
/// fields, nested spans): the order of a type's fields is the order of its
/// JSON.
#[test]
fn json_interned_without_a_value_matches_the_value_route() {
    let sources = [
        ("a.rs", include_str!("../tables.rs")),
        ("b.rs", include_str!("../tables/arguments.rs")),
        (
            "c.ts",
            "import {a as b} from './x';\nexport class K<T> extends Base { m(x: T) { return b(x).y?.z(1, 'two', ...rest); } }\n",
        ),
        (
            "d.py",
            "import os\nclass A(B):\n    def f(self, x, *a, k=1):\n        return os.path.join(x, k, 'é')\n",
        ),
        ("e.go", "package p\nfunc F(x int) { G(x, func() {}) }\n"),
    ];
    let mut interner = Interner::default();
    let mut checked = 0;
    let mut check = |value: &dyn both_routes::Json| {
        let id = value.intern(&mut interner);
        assert_eq!(interner.strings[id as usize], value.via_value());
        checked += 1;
    };
    for (file, source) in sources {
        let facts = octocode_engine::signatures::extract_graph_facts(source, file).expect("facts");
        for declaration in &facts.declarations {
            check(&both_routes::Of(&declaration.test_identity));
            check(&both_routes::Of(&declaration.parse_assessment));
        }
        for site in &facts.argument_sites {
            check(&both_routes::Of(site));
            for group in &site.groups {
                check(&both_routes::Of(&group.observation));
            }
        }
        for call in &facts.calls {
            check(&both_routes::Of(call));
        }
    }
    assert!(checked > 100, "{checked} values compared");
}

/// Both routes of one value, behind one type.
mod both_routes {
    use super::Interner;
    use serde::Serialize;

    pub(super) trait Json {
        fn intern(&self, interner: &mut Interner) -> u32;
        fn via_value(&self) -> String;
    }

    pub(super) struct Of<'a, T>(pub(super) &'a T);

    impl<T: Serialize> Json for Of<'_, T> {
        fn intern(&self, interner: &mut Interner) -> u32 {
            interner.json(self.0)
        }
        fn via_value(&self) -> String {
            serde_json::json!(self.0).to_string()
        }
    }
}

#[test]
fn package_names_follow_ecosystem_rules() {
    assert_eq!(
        package_name("@scope/pkg/sub", "typescript").as_deref(),
        Some("@scope/pkg")
    );
    assert_eq!(
        package_name("react-dom/client", "javascript").as_deref(),
        Some("react-dom")
    );
    assert_eq!(
        package_name("serde::Deserialize", "rust").as_deref(),
        Some("serde")
    );
    assert_eq!(package_name("os.path", "python").as_deref(), Some("os"));
    assert_eq!(
        package_name("github.com/a/b/c", "go").as_deref(),
        Some("github.com/a/b")
    );
    assert_eq!(package_name("fmt", "go").as_deref(), Some("fmt"));
    assert_eq!(
        package_name("java.util.List", "java").as_deref(),
        Some("java.util")
    );
    assert_eq!(package_name("./local", "typescript"), None);
}

#[test]
fn callee_split_handles_member_and_path_calls() {
    assert_eq!(split_callee("run"), ("", "run"));
    assert_eq!(split_callee("this.run"), ("this", "run"));
    assert_eq!(split_callee("Self::new"), ("Self", "new"));
    assert_eq!(split_callee("a.b.c"), ("a.b", "c"));
    assert_eq!(first_segment("utils::inner"), "utils");
}

/// A minified bundle repeats one name on one line thousands of times. Its
/// keys stay unique and deterministic (`name@1`, `name@2`, … in source
/// order), with no suffix probe: linear in the declarations.
#[test]
fn same_name_repeats_on_one_line_get_unique_keys_in_linear_work() {
    let dir = octocode_runtime_base::test_support::Fixture::new();
    dir.write("x.js", "var a=1;\n");
    let built = crate::ast_graph::graph::build_graph(
        &crate::ast_graph::graph::GraphScan {
            root: dir.0.to_path_buf(),
            ..Default::default()
        },
        &octocode_runtime_base::test_support::workspace_policy(dir.0.as_path()),
        &octocode_runtime_base::security::ContentSecurity::new(),
        &octocode_runtime_base::execution::cancel::NeverCancel,
    )
    .expect("built");
    let template = built.facts["x.js"].declarations[0].clone();
    let repeats = 100_000;
    let declarations = (0..repeats)
        .map(|index| Declaration {
            id: format!("decl-{index}"),
            ..template.clone()
        })
        .collect::<Vec<_>>();
    let ordered = declarations.iter().collect::<Vec<_>>();
    let by_id = declarations
        .iter()
        .map(|decl| (decl.id.as_str(), decl))
        .collect::<HashMap<_, _>>();
    KEY_PROBES.with(|probes| probes.set(0));
    let keys = symbol_keys("x.js", &ordered, &by_id);
    let probes = KEY_PROBES.with(std::cell::Cell::get);
    assert!(
        probes < 2 * repeats,
        "{probes} suffix probes for {repeats} repeats"
    );
    assert_eq!(keys[0], "x.js#a@1");
    assert_eq!(keys[1], "x.js#a@2");
    assert_eq!(keys[repeats - 1], format!("x.js#a@{repeats}"));
    assert_eq!(keys.iter().collect::<BTreeSet<_>>().len(), repeats);
}

/// A linkage ratio is a share, not a measurement: it keeps 4 significant
/// digits, each within rounding of linked / eligible.
#[test]
fn a_linkage_ratio_keeps_four_significant_digits() {
    for linked in 1..40_u64 {
        for missed in 0..40_u64 {
            let stats = CallStats {
                linked,
                unresolved: missed,
                unresolved_by_reason: BTreeMap::from([("other", missed)]),
                ..CallStats::default()
            };
            let linkage = stats.linkage();
            let ratio = linkage["ratio"].as_f64().expect("ratio");
            let exact = linked as f64 / (linked + missed) as f64;
            assert_eq!(
                format!("{ratio:.3e}").parse::<f64>().ok(),
                Some(ratio),
                "{linkage}"
            );
            assert!((ratio - exact).abs() <= exact * 5e-4, "{linkage}");
        }
    }
}
