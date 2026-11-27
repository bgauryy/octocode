//! The one-pass nesting bound: never below the depth of the syntax tree
//! where it vouches for a file, unsure where oxc's reading could differ, and
//! vouching for ordinary sources so they skip the tree.

use super::super::nesting::{MAX_NESTING, pass_bound, syntax_depth};
use super::nesting::{CONSTRUCTS, reps_at_limit};

fn holds(source: &str, ext: &str) -> Option<(usize, usize)> {
    let bound = pass_bound(source, ext)?;
    let depth = syntax_depth(source, ext).expect("measured").0;
    assert!(bound >= depth, "{ext}: bound {bound} < depth {depth}");
    Some((bound, depth))
}

#[test]
fn the_bound_is_never_below_the_tree_depth_of_any_way_to_nest() {
    for construct in CONSTRUCTS {
        let at_limit = reps_at_limit(construct);
        for reps in [1, 10, 100, at_limit / 4, at_limit / 2] {
            holds(&(construct.source)(reps), construct.ext);
        }
        // Past the limit the pass stops early: the caller measures.
        let deep = (construct.source)(at_limit * 2);
        assert_eq!(pass_bound(&deep, construct.ext), None, "{}", construct.name);
    }
}

/// Small sources where the tree sits deepest relative to the tokens: a lone
/// comment or name, statements without `;`, every literal and JSX form.
#[test]
fn the_bound_holds_on_shallow_shapes_of_every_kind() {
    for (ext, source) in [
        ("ts", "// only a comment\n"),
        ("ts", "// a comment ends at a lone CR\r`a\r${(b)}\r`"),
        ("js", "// or at U+2028\u{2028}f((x))"),
        ("ts", "x"),
        ("ts", "a\nb\nc"),
        ("js", "/* c */ f(/re[)]/g, `a${`b${c}`}`, 'q', \"r\")"),
        (
            "tsx",
            "const v = <A b=\"1\" {...p}><B/>text {x && <C>{y}</C>}</A>;",
        ),
        ("tsx", "const f = <></>;"),
        ("ts", "type T = A<B<C>> | { a: D[]; b(): E };"),
        ("ts", "class K { #p = 1; static { if (a) b(); else c(); } }"),
        ("js", "do x(); while (y)\nfor (const a of b) /x/.test(a);"),
        (
            "js",
            "#!/usr/bin/env node\nlabel: for (;;) { break label; }",
        ),
    ] {
        holds(source, ext).unwrap_or_else(|| panic!("{ext}: {source}"));
    }
}

#[test]
fn ordinary_code_far_longer_than_the_limit_is_vouched_for() {
    let unit = "export function f(a, b) {\n  if (a < b && /x[)]/.test(`${a}`)) { return [a, { b }]; }\n  const c = a / b; // ) ]\n  return c > 1 ? f(c, b) : `t ${c} }`;\n}\n";
    let ts = unit.repeat(MAX_NESTING / unit.len() * 4);
    assert!(ts.len() > MAX_NESTING * 3);
    holds(&ts, "ts").expect("vouched");
    let tsx = format!(
        "{}{}",
        ts,
        "const v = <A b=\"(\" c={x / 2}>t ( {[1].map(y => <B key={y}/>)} </A>;\n"
            .repeat(MAX_NESTING / 40)
    );
    holds(&tsx, "tsx").expect("vouched");
}

/// Where oxc's reading depends on context the pass does not track, it does
/// not vouch: the caller measures the tree.
#[test]
fn readings_that_depend_on_context_are_left_to_the_tree() {
    let pad = "x;\n".repeat(MAX_NESTING);
    for (ext, tail) in [
        ("js", "if (a) {}\n/((/.test(b)\n"),
        ("ts", "x! / ((y) / z\n"),
        ("tsx", "const f = <T,>(v: T) => v;\n"),
        ("tsx", "const f = <T extends U>(v: T) => v;\n"),
        ("js", "await /((/;\n"),
        ("js", "a <!-- b\n"),
        ("js", "a++ / b / c\n"),
        ("js", "'unterminated\n"),
    ] {
        assert_eq!(pass_bound(&format!("{pad}{tail}"), ext), None, "{tail}");
    }
}
