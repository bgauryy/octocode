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
