# oxc toolkit: use the built-ins before hand-rolling

Load when writing JS/TS analysis on oxc: imports and exports, comments or JSDoc, bindings and globals, node comparison or construction, or many checks per file. Why: oxc already computes most facts that people hand-roll with a walk, and its arena types have ownership rules that break when misused.

## Cargo prerequisites (the engine currently has none of these)

| Want | Needs |
|---|---|
| `Visit`, `VisitMut`, `VisitJs` | `oxc_ast_visit` dependency |
| `Utf8ToUtf16` | `oxc_ast_visit` with feature `serialize` |
| `AllocatorPool` | `oxc_allocator` with feature `pool` |
| `AstNodes::contains_any`, `Semantic::jsdoc()` | `oxc_semantic` feature `linter` / `jsdoc` |
| `Traverse` | `oxc_traverse` dependency |

A new dependency or feature needs consent (see the `rust-best-practices` rules).

## Facts available without walking

| Need | Use | Don't |
|---|---|---|
| imports / exports / dynamic `import()` / `import.meta` | `ParserReturn.module_record` (`requested_modules`, `import_entries`, `local_export_entries`, `indirect_export_entries`, `star_export_entries`, `dynamic_imports`; entries carry `statement_span`, `is_type`) | a `Visit` over every statement |
| comments | `program.comments` (sorted, filled by the parser, no semantic pass needed). Leading comments of a node: `c.is_leading() && c.attached_to == node.span.start`. Helpers: `is_jsdoc`, `is_legal`, `is_pure`, `content_span()`; `trivia::comments_range`, `has_comments_between` | regex over the source |
| JSDoc for a node | `semantic.jsdoc().get_one_by_node(node)` (needs `AstNodes` + `jsdoc` feature) | parsing `/** */` by hand |
| globals / free variables | `scoping.root_unresolved_references()`, `semantic.is_reference_to_global_variable(..)` | name matching against a list |
| binding kind | `SymbolFlags` (`is_value/is_type/is_variable`, `Import`, `TypeImport`, `Function`, `Class`…), `ReferenceFlags::is_read/is_write/is_type` | inferring from the parent node's shape |
| names a pattern binds, static keys, truthiness | `oxc_ecmascript` traits: `BoundNames`, `PropName`, `ToBoolean`, `ConstantValue` | a hand-written destructuring walker |
| `.span()` on anything | `GetSpan` (derived for every node and `AstKind`) | matching per variant |

## Ownership and identity

- **Strings.** oxc 0.150 has `Str<'a>` (an arena `&str`) and `Ident<'a>` (with a precomputed hash; equality checks the hash first). There is no `Atom`. Keep `&'a str`/`Ident` while the arena is alive, and convert to owned `CompactStr`/`String` only at output. `to_compact_str()` stays inline up to 16 bytes and allocates beyond that. `Scoping::find_binding(scope, name)` takes an `Ident`.
- **Comparing nodes.** `ContentEq::content_eq` ignores spans, so use it for "same expression" checks and dedup. Don't compare spans or printed text.
- **Copying nodes.** `CloneIn::clone_in(&alloc)` deep-copies and **drops semantic ids**. `clone_in_with_semantic_ids` keeps them. `TakeIn` moves a node out of `&mut`.
- **Building nodes.** Store an `AstBuilder::new(&allocator)` in a `VisitMut` visitor. `Traverse` supplies one as `ctx.ast`. `ctx.generate_uid` keeps scoping in sync.
- **NodeId.** Ids are assigned in both builder modes, but `nodes().kind(id)`, `parent_id`, `get_node` and `symbol_declaration` **panic on the index** unless the builder had `with_build_nodes(true)`.
- **`SemanticBuilder` presets.** `new()` is the base. `new_compiler()` adds `with_check_syntax_error`. `new_linter()` adds nodes, CFG, class table and JSDoc.

## Many checks, one pass (oxc linter pattern)

1. Build `Semantic` once with `with_build_nodes(true)`.
2. Skip a file cheaply with `nodes().contains_any(&bitset_of_types_my_checks_need)`.
3. Put the checks into buckets by `AstType`, then run **one flat loop** `for node in semantic.nodes()` that dispatches `buckets[node.kind().ty()]`. There is no recursion per check, and the loop itself can't overflow the stack.
4. Run once-per-file checks (scopes, comments, `module_record`) outside the loop.

The oxc linter reruns the unoptimized path in debug builds and asserts the results match. Copy that differential check when you add bucketing.

Next: walking when a flat loop won't do → `references/walking-oxc-and-depth.md`; rewrites → `references/editing.md`.
