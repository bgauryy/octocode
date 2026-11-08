//! The cursor walk behind `has` visits exactly the nodes, in exactly the
//! order, of the node-list walk it replaced, for every field and stop.

use std::num::NonZeroU16;

use tree_sitter::Node;

use super::{Visit, walk_descendants};
use crate::signatures::languages;
use crate::structural::language::AgLanguage;
use crate::structural::octo::matching::{named_children, parse_tree};

/// The walk the cursor replaced: a stack filled with each visited node's
/// collected named children.
fn listed_walk<'t>(
    candidate: Node<'t>,
    field: Option<NonZeroU16>,
    mut visit: impl FnMut(Node<'t>) -> Visit,
) -> bool {
    let mut stack = if let Some(field) = field {
        candidate
            .children_by_field_id(field, &mut candidate.walk())
            .filter(Node::is_named)
            .collect::<Vec<_>>()
    } else {
        named_children(candidate)
    };
    stack.reverse();
    while let Some(node) = stack.pop() {
        match visit(node) {
            Visit::Found => return true,
            Visit::Descend => stack.extend(named_children(node).into_iter().rev()),
            Visit::Skip => {}
        }
    }
    false
}

/// Every named node of the tree, in pre-order.
fn every_named_node(root: Node<'_>) -> Vec<Node<'_>> {
    let mut nodes = vec![root];
    listed_walk(root, None, |node| {
        nodes.push(node);
        Visit::Descend
    });
    nodes
}

/// Visit policies that stop and end the walk at varied places.
fn policies() -> [fn(Node<'_>) -> Visit; 5] {
    [
        |_| Visit::Descend,
        |_| Visit::Skip,
        |node| {
            if node.kind_id() % 2 == 0 {
                Visit::Skip
            } else {
                Visit::Descend
            }
        },
        |node| {
            if node.named_child_count() > 2 {
                Visit::Skip
            } else {
                Visit::Descend
            }
        },
        |node| {
            if node.start_byte() % 7 == 3 {
                Visit::Found
            } else if node.kind_id() % 3 == 0 {
                Visit::Skip
            } else {
                Visit::Descend
            }
        },
    ]
}

const FIXTURES: &[(&str, &str)] = &[
    (
        "js",
        "// head\nfunction f(a, { b = 1 }, ...rest) {\n  /* in */ return g(a, `t${b}`, [1, , 3], x => x?.y ?? z);\n}\nclass C extends D { #p = 1; static m() { return new C(); } }\nconst o = { k: 1, [k2]: 2, ...s };\nif (a) b(); else { c(); }\n",
    ),
    (
        "ts",
        "export interface I<T> { a: T; b?(x: number): void }\nfunction h<T extends I<number>>(x: T): T { return x as T; }\nenum E { A = 1, B }\nlet v = (y: string) => { for (const k of y) { if (!k) break; } };\nconst bad = (;\n",
    ),
    (
        "rs",
        "#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() { let v = w.unwrap(); assert_eq!(v, 1); }\n}\nimpl<T: Clone> S<T> where T: Copy { fn m(&self) -> Option<T> { match self.0 { Some(x) => Some(x), None => None } } }\nmacro_rules! m { ($x:expr) => { $x }; }\n",
    ),
    (
        "py",
        "@decorator\ndef f(a, *args, b=1, **kw):\n    \"\"\"doc\"\"\"\n    return [x for x in a if x], {k: v for k, v in kw.items()}\n\nclass C(B):\n    def m(self):\n        with open(p) as fh:\n            pass\n",
    ),
];

#[test]
fn cursor_walk_visits_what_the_node_list_walk_visited() {
    for (ext, source) in FIXTURES {
        let lang = AgLanguage::new(languages::find_entry(ext).expect("fixture language"));
        let language = lang.tree_sitter_language();
        let tree = parse_tree(&language, source).expect("fixture parses");
        let fields = std::iter::once(None).chain(
            (1..=language.field_count())
                .filter_map(|id| NonZeroU16::new(u16::try_from(id).expect("field id fits u16")))
                .map(Some),
        );
        let fields = fields.collect::<Vec<_>>();
        let mut compared = 0_usize;
        for candidate in every_named_node(tree.root_node()) {
            for &field in &fields {
                for policy in policies() {
                    let mut listed = Vec::new();
                    let listed_found = listed_walk(candidate, field, |node| {
                        listed.push(node.id());
                        policy(node)
                    });
                    let mut walked = Vec::new();
                    let walked_found = walk_descendants(candidate, field, |node| {
                        walked.push(node.id());
                        Ok(policy(node))
                    })
                    .expect("no deadline");
                    assert_eq!(
                        (walked_found, &walked),
                        (listed_found, &listed),
                        "{ext}: candidate {} at byte {}, field {field:?}",
                        candidate.kind(),
                        candidate.start_byte(),
                    );
                    compared += listed.len();
                }
            }
        }
        assert!(compared > 0, "{ext}: the fixture exercised no visits");
    }
}

/// The nodes `rule` accepts in `source`, by span and kind, and the rule
/// evaluations it took, with the file's relation memo or without.
fn accepted(
    rule: &serde_json::Value,
    ext: &str,
    source: &str,
    kept: bool,
) -> (Vec<(usize, usize, u16)>, usize) {
    let (found, evaluations, _) = accepted_keeping(rule, ext, source, kept);
    (found, evaluations)
}

/// [`accepted`], and the answers the file's relation memo kept.
fn accepted_keeping(
    rule: &serde_json::Value,
    ext: &str,
    source: &str,
    kept: bool,
) -> (Vec<(usize, usize, u16)>, usize, usize) {
    use crate::structural::octo::matching::{CaptureEnv, Deadline, visit_named_expanding_macros};
    use crate::structural::octo::rule::{CompiledRule, Document, EVALUATIONS, RelationMemo};
    let lang = AgLanguage::new(languages::find_entry(ext).expect("language"));
    let language = lang.tree_sitter_language();
    let tree = parse_tree(&language, source).expect("parses");
    let rule = CompiledRule::new(&lang, rule).expect("rule");
    let line_index = crate::text::LineIndex::tree_sitter(source);
    let memo = RelationMemo::of(tree.root_node());
    let before = EVALUATIONS.with(std::cell::Cell::get);
    let mut found = Vec::new();
    visit_named_expanding_macros(
        tree.root_node(),
        source,
        &language,
        None,
        Deadline::UNBOUNDED,
        &mut |candidate, ancestors, root, _| {
            let document = Document {
                memo: kept.then_some(&memo),
                content: source,
                deadline: Deadline::UNBOUNDED,
                root,
                line_index: &line_index,
            };
            let mut captures = CaptureEnv::default();
            if rule
                .matches(candidate, Some(ancestors), &document, &mut captures)
                .expect("no deadline")
            {
                found.push((
                    candidate.start_byte(),
                    candidate.end_byte(),
                    candidate.kind_id(),
                ));
            }
            Ok(())
        },
    )
    .expect("no deadline");
    let evaluations = EVALUATIONS.with(std::cell::Cell::get) - before;
    (found, evaluations, memo.answers.borrow().len())
}

/// Only a relation whose answer for a node is the same whoever asks keeps
/// it: one that binds or compares a capture, reads an ordinal (`nthChild`),
/// looks through one field, or stops before the file's edge keeps nothing,
/// and accepts what its walk accepts. Each kept twin (the same relation
/// without that part) keeps answers, so an empty memo is the exclusion.
#[test]
fn relations_whose_answer_depends_on_the_asker_keep_nothing() {
    let (ext, source) = FIXTURES[0];
    let end = |relation: serde_json::Value| {
        let mut relation = relation;
        relation["stopBy"] = serde_json::json!("end");
        relation
    };
    let excluded = [
        (
            "capture",
            serde_json::json!({"pattern": "$A", "stopBy": "end"}),
        ),
        (
            "nthChild",
            serde_json::json!({"kind": "identifier", "nthChild": 1, "stopBy": "end"}),
        ),
        (
            "field",
            serde_json::json!({"kind": "identifier", "field": "name", "stopBy": "end"}),
        ),
        ("stopBy neighbor", serde_json::json!({"kind": "identifier"})),
        (
            "stopBy rule",
            serde_json::json!({"kind": "identifier", "stopBy": {"kind": "statement_block"}}),
        ),
    ];
    for (part, relation) in excluded {
        for direction in ["has", "inside"] {
            let rule = serde_json::json!({ direction: relation.clone() });
            let (kept, _, answers) = accepted_keeping(&rule, ext, source, true);
            let (walked, _, _) = accepted_keeping(&rule, ext, source, false);
            assert_eq!(answers, 0, "{part} {direction}: {rule}");
            assert_eq!(kept, walked, "{part} {direction}: {rule}");
        }
    }
    let twins = [
        serde_json::json!({"kind": "identifier"}),
        serde_json::json!({"kind": "statement_block"}),
    ];
    for relation in twins {
        for direction in ["has", "inside"] {
            let rule = serde_json::json!({ direction: end(relation.clone()) });
            let (_, _, answers) = accepted_keeping(&rule, ext, source, true);
            assert!(answers > 0, "{direction}: {rule} keeps its answers");
        }
    }
}

/// Keeping what `has` and `inside` settle changes no answer: every rule over
/// every kind of each fixture accepts the same nodes with and without it.
#[test]
fn kept_relation_answers_accept_what_the_walks_accept() {
    let mut compared = 0_usize;
    for (ext, source) in FIXTURES {
        let lang = AgLanguage::new(languages::find_entry(ext).expect("fixture language"));
        let tree = parse_tree(&lang.tree_sitter_language(), source).expect("fixture parses");
        let mut kinds = every_named_node(tree.root_node())
            .into_iter()
            .map(|node| node.kind().to_owned())
            .collect::<Vec<_>>();
        kinds.sort();
        kinds.dedup();
        for kind in &kinds {
            let rules = [
                serde_json::json!({"has": {"kind": kind, "stopBy": "end"}}),
                serde_json::json!({"inside": {"kind": kind, "stopBy": "end"}}),
                serde_json::json!({"all": [
                    {"has": {"kind": kind, "stopBy": "end"}},
                    {"not": {"inside": {"kind": kind, "stopBy": "end"}}},
                ]}),
                serde_json::json!({"any": [
                    {"inside": {"has": {"kind": kind, "stopBy": "end"}, "stopBy": "end"}},
                    {"has": {"inside": {"kind": kind, "stopBy": "end"}, "stopBy": "end"}},
                ]}),
            ];
            for rule in rules {
                let (walked, _) = accepted(&rule, ext, source, false);
                let (kept, _) = accepted(&rule, ext, source, true);
                assert_eq!(kept, walked, "{ext}: {rule}");
                compared += walked.len();
            }
        }
    }
    assert!(compared > 0, "no rule accepted a node");
}

/// A nested node starts its own walk through the levels below (or above) it;
/// twice the nesting must cost about twice the evaluations, not four times,
/// whether the walk ends at the far end or finds nothing.
#[test]
fn walks_through_every_level_cost_linear_work_in_the_nesting() {
    let nested = |depth: usize| format!("{}1{}", "[".repeat(depth), "]".repeat(depth));
    let rules = [
        serde_json::json!({"kind": "array", "has": {"kind": "number", "stopBy": "end"}}),
        serde_json::json!({"kind": "array", "has": {"kind": "string", "stopBy": "end"}}),
        serde_json::json!({"kind": "array", "inside": {"kind": "string", "stopBy": "end"}}),
        serde_json::json!({"kind": "number", "inside": {"kind": "program", "stopBy": "end"}}),
        serde_json::json!({"kind": "array", "has": {"pattern": "[$$$]", "stopBy": "end"}}),
    ];
    for rule in rules {
        let (_, work_small) = accepted(&rule, "ts", &nested(300), true);
        let (_, work_large) = accepted(&rule, "ts", &nested(600), true);
        assert!(
            work_large * 10 <= work_small * 25,
            "{rule}: {work_small} evaluations at 300 levels, {work_large} at 600"
        );
        let (walked, _) = accepted(&rule, "ts", &nested(60), false);
        let (kept, _) = accepted(&rule, "ts", &nested(60), true);
        assert_eq!(kept, walked, "{rule}");
    }
}
