# oxc visitors and depth guards

Load when walking an oxc AST, or when any walk recurses over user-controlled nesting. Why: hand-rolled `match` walkers silently miss node shapes, and oxc recursion has no built-in depth guard.

## Visitors

```rust
struct Calls<'s> { out: Vec<Span>, _s: &'s () }
impl<'a> Visit<'a> for Calls<'_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        self.out.push(it.span);                 // pre-order work
        walk::walk_call_expression(self, it);   // omit this line to prune; code after it runs post-order
    }
}
```

- The visitor lives in `oxc_ast_visit`, which the engine doesn't depend on yet (see `references/oxc-toolkit.md`). `VisitJs` is the same visitor but skips TypeScript type-space nodes, which suits JS-only analysis of `.ts` files.
- Override only the node types you care about. The generated `walk_*` functions reach every nested expression, including ones a hand-written `match` forgets. Hand-rolled `match stmt { … _ => {} }` walkers silently miss new node shapes; `signatures/js_oxc_calls.rs` has documented dropped-call bugs from exactly this.
- For generic tracking, use `enter_node(AstKind)`/`leave_node` and `enter_scope`/`leave_scope`. oxc's `enter_node` can't prune. Ruff's `SourceOrderVisitor` returns `TraversalSignal::{Traverse, Skip}` from `enter_node`, which is the shape to copy when you write your own generic walker.
- **When no walk is needed at all:** with `AstNodes` built, one flat `for node in semantic.nodes()` loop visits everything with no recursion. Bucket checks by `AstType` (see `references/oxc-toolkit.md`).
- `Traverse` is the only safe way to mutate while reading ancestors. `Ancestor` says both the parent type and which field you came from. Don't hand-edit the generated traversal code.

## Depth and termination

- oxc parse, `Visit` and codegen are recursive with **no depth guard**. Run them on a large-stack thread (house: `signatures/deep_stack.rs`, 64 MB, with `catch_unwind`), or cap nesting before parsing.
- A recursive matcher over tree-sitter needs an explicit depth cap. House caps: pattern depth 500, rule nesting 64, 10k multi-capture attempts.
- Iterative cursor walks need no depth cap, only a deadline.

Next: matching shapes rather than walking → `references/tree-sitter-queries.md` (queries) or `references/patterns-and-rules.md` (ast-grep/octo).
