# Walking

Load when writing or fixing a traversal. Why: recursive walks overflow on hostile nesting, `child(i)` loops go quadratic, and a forgotten `walk_*` call silently prunes a whole subtree.

## Choose the walk

| Need | tree-sitter | oxc |
|---|---|---|
| Visit every node, read-only | cursor preorder loop (below) | `impl Visit<'a>` (`oxc_ast_visit`) |
| Pre- and post-order hooks (scope push/pop) | explicit stack of `Enter(node)`/`Exit(node)` frames | override `visit_x`: work → `walk::walk_x(self, it)` → work |
| Prune a subtree | skip `goto_first_child`, go to the sibling | don't call `walk::walk_x` |
| Local rewrite, no parent needed | edit text by span (see editing) | `impl VisitMut<'a>` |
| Rewrite that needs ancestors or scope | not applicable | `oxc_traverse::Traverse`: `enter_*`/`exit_*` with `ctx.parent()` and `ctx.ancestor(n)` |
| Parent or ancestor lookup | keep your own stack during the walk | `AstNodes::parent_id` / `ancestor_ids` (needs `with_build_nodes(true)`) |
| Only certain kinds | kind prefilter (`kind_id` set), or a `Query` | `visit_<that_type>` only |

## tree-sitter: cursor preorder with no allocation and no recursion

```rust
fn visit_named<'t>(root: Node<'t>, deadline: Instant,
                   f: &mut impl FnMut(Node<'t>) -> ControlFlow<()>) -> Result<(), Timeout> {
    let mut c = root.walk();
    loop {
        if Instant::now() >= deadline { return Err(Timeout); }
        let n = c.node();
        let descend = !n.is_named() || f(n).is_continue(); // Break = prune this subtree
        if descend && c.goto_first_child() { continue; }
        loop {
            if c.goto_next_sibling() { break; }
            if !c.goto_parent() { return Ok(()); }   // back at root: done
        }
    }
}
```

- One cursor, O(1) per step. The cursor cannot move above `root`.
- The house version is `structural/octo/matching.rs::visit_named`. Reuse it; don't write another.
- Checking the deadline on every node is cheap. Use a counter if a profile says otherwise.
- `goto_last_child` and `goto_previous_sibling` are slower than their forward equivalents. Walk forward.

**Children of one node:** `node.named_children(&mut cursor)`, with the cursor reused across calls. Never write `for i in 0..node.child_count() { node.child(i) }`: each `child(i)` is O(i) in the C code (`ts_node__child` scans linearly), even though the Rust doc says "technically log(i)".

**Ancestors:** `node.parent()` searches down from the root on every call. Keep your own ancestor stack while walking, or use `root.child_with_descendant(node)` step by step as the 0.27 docs recommend.

**Enter/exit with an explicit stack** (for scope tracking and post-order facts). This is the house pattern in `signatures/graph_facts/mod.rs`:

```rust
enum Frame<'t> { Enter(Node<'t>), Exit(Node<'t>) }
let mut stack = vec![Frame::Enter(root)];
let mut kids = Vec::new(); let mut cur = root.walk();
while let Some(fr) = stack.pop() {
    match fr {
        Frame::Enter(n) => { on_enter(n); stack.push(Frame::Exit(n));
            kids.clear(); kids.extend(n.named_children(&mut cur));
            stack.extend(kids.drain(..).rev().map(Frame::Enter)); }
        Frame::Exit(n) => on_exit(n),
    }
}
```

Next: oxc visitors, depth guards and termination → `references/walking-oxc-and-depth.md`; matching shapes rather than walking → `references/tree-sitter-queries.md` (queries) or `references/patterns-and-rules.md` (ast-grep/octo).
