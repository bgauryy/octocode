# AST Best Practices

Correct, bounded and efficient syntax-tree code in Rust, for **tree-sitter 0.27**, **oxc 0.150** and **ast-grep-core 0.45**, as used by the `octocode-native` engine.

## Use when

- Writing or reviewing a walker, visitor, cursor loop or ancestor lookup.
- Holding node handles or ids: what they borrow and what survives an edit.
- Writing tree-sitter queries, ast-grep rules or patterns, or octo structural patterns.
- Converting positions (bytes, UTF-16, 0- vs 1-based lines, CRLF, BOM).
- Choosing a grammar or oxc `SourceType`, or a kind name that silently matches nothing.
- Testing tree code against adversarial input.
- Rewriting code by byte range, or doing incremental reparse.
- Fixing a parse or walk that is slow, allocates heavily, times out or overflows the stack.

Not for calling astSearch or astRewrite as a user (use `octocode-research`), or for general Rust idioms (use `rust-best-practices`).

## Layout

| File | Job |
|---|---|
| `SKILL.md` | lobby: mental model, stack picker, numbered core rules, stop/escalate, review format, routes |
| `references/grammar-and-language.md` | ABI, tsx vs ts, kind-name traps, aliases, 0.27 API changes, injections, oxc SourceType/ParseOptions |
| `references/oxc-toolkit.md` | oxc built-ins (module_record, comments, Scoping, ecmascript), ownership, one-pass linter pattern, Cargo features |
| `references/testing.md` | fixture classes, snapshots, round-trip, differential and fuzz testing |
| `scripts/adversarial-fixtures.mjs` | writes the adversarial fixtures (`--help`) |
| `references/concepts-and-handles.md` | CST/AST, node kinds, errors, per-stack handle table, position units |
| `references/walking.md` | walk choice, tree-sitter cursor and enter/exit loops |
| `references/walking-oxc-and-depth.md` | oxc `Visit`/`VisitMut`/`Traverse`, depth guards |
| `references/tree-sitter-queries.md` | `Query`/`QueryCursor` bounds, predicates, operators, errors |
| `references/patterns-and-rules.md` | ast-grep strictness, metavariables, rule order, constraints; octo matcher |
| `references/editing.md` | span-splice rewrite protocol, oxc codegen vs splice, incremental reparse |
| `references/efficiency.md` | reuse table, bounds, allocation cuts, parallelism |
| `references/octocode-engine-map.md` | which engine helper to reuse, position contract per surface |
| `references/octocode-known-defects.md` | guards and tests for fixed engine defects; open watch items |
| `references/references.md` | the sources behind every claim |
