# Concepts and handles

Load when you need to know what a node is, what a handle to it is worth, or what unit a position is in. Why: most AST bugs are identity bugs (a stale handle, an id used as a key) or unit bugs (bytes vs UTF-16, 0- vs 1-based).

## CST vs AST

| | tree-sitter | oxc | ast-grep |
|---|---|---|---|
| Tree kind | **Concrete (CST)**: every token is a node; comments are "extra" nodes that can appear anywhere | **Abstract (AST)**: typed Rust structs; comments sit in `program.comments`, sorted and filled by the parser (`Semantic` mirrors them) | tree-sitter CST underneath, matched with strictness levels |
| Node type | dynamic `kind()` string or `kind_id()` u16 | static enum or struct, plus `AstKind` for untyped access | `kind()` string |
| Storage | C heap, refcounted `Tree` (`clone` is cheap) | bump arena `Allocator`; nothing is dropped, memory is freed in bulk | owns a tree-sitter tree through `Root<D>` |

- **Named vs anonymous.** Named nodes come from grammar rules (`identifier`). Anonymous nodes come from string literals in the grammar (`(`, `;`, `+`). Use `named_child*` and `is_named()` to skip punctuation, and walk all children only when punctuation matters, as it does in exact pattern matching.
- **Fields.** Grammar field names such as `name`, `body` and `arguments` are more stable than child positions. Use `child_by_field_name`, or `child_by_field_id` with an id resolved once through `language.field_id_for_name`.
- **Error nodes.**
  - `is_error()` means text the parser could not place.
  - `is_missing()` means a zero-width token the parser inserted to recover.
  - `has_error()` is true for a subtree that contains either.
  - A tree always comes back; it isn't necessarily valid.
- **oxc errors.** `ParserReturn { program, diagnostics, fatal_error, .. }`. A recoverable error gives a full program plus diagnostics. `fatal_error: true` means the program is empty. Some early errors are only reported by `SemanticBuilder::with_check_syntax_error(true)`.

## Handles per stack

| Handle | Lifetime and cost | Survives an edit? | Use for |
|---|---|---|---|
| ts `Node<'tree>` | `Copy`; borrows `Tree` | **No**. `Tree::edit` needs `&mut Tree`; call `Node::edit` to shift a node you kept | in-walk work |
| ts `node.id()` | unique within **one** tree | only for nodes an incremental reparse reused | per-walk memo keys; never persisted |
| ts `TreeCursor` | stays inside its start node; `reset(node)` reuses it | no | iterative walks, `children(&mut cursor)` |
| ts `node.parent()` | **no stored parent pointer**; searches down from the root | no | rare lookups; keep the ancestor stack yourself |
| oxc `&'a T` (`Box<'a,_>`, `Vec<'a,_>`) | borrows the `Allocator` | arena reset frees everything | typed access |
| oxc `NodeId` | set on each node's `node_id: Cell<NodeId>` by `SemanticBuilder`, in every mode | same `Semantic` only | `nodes.kind(id)`, `parent_id(id)`. These **panic** unless the builder had `with_build_nodes(true)` |
| oxc `SymbolId` / `ReferenceId` / `ScopeId` | indices into `Scoping` | same `Semantic` | binding facts: `symbol_name`, `get_resolved_reference_ids`, `symbol_is_unused`, `scope_ancestors` |
| ast-grep `Node<'r,D>` | borrows `Root` | no; `Root::edit` reparses | `find_all`, `inside`, `has`, `field`, `ancestors` |
| ast-grep `NodeMatch` | node + `MetaVarEnv` | no | `get_env().get_match("A")`, `get_multiple_matches("ARGS")` for `$$$ARGS` |

**Durable identity.**
- `(path, start_byte, end_byte, kind)` is rust-analyzer's `SyntaxNodePtr` model. It resolves only against the **same content hash**; any edit can invalidate it. Store the hash alongside it.
- For ids that survive edits, anchor on the item (kind + name + a disambiguator under its parent, like rust-analyzer's `AstIdMap`), store positions relative to that anchor, or use a domain id (`file:…`, `symbol:…`, see `docs/engine/CODE_GRAPH.md`).
- Tree ids are scratch values.

**Drop the rest of `Semantic` early.** `into_scoping()` or `into_scoping_and_nodes()` keep only what you need.

## Position units

| Stack | Offset | Line / column |
|---|---|---|
| tree-sitter | `byte_range()`: bytes | `Point{row,column}`: 0-based row, **byte** column |
| oxc | `Span{start,end}`: `u32` bytes, so the file cap is 4 GiB | none built in. `Utf8ToUtf16` (feature `serialize`) is available: `convert_program` **rewrites spans in place** and destroys byte offsets, so use `.converter()` for lookups |
| ast-grep | `range()`: bytes | `start_pos()`: 0-based line; column is a **char** count (`column(&node)`) |
| LSP / JS / VS Code | UTF-16 code units | 0-based line and UTF-16 column |
| ripgrep JSON | bytes | 1-based line |

Rules:
- Store bytes. Convert once through a shared `LineIndex` that keeps line-start tables for bytes and UTF-16 and snaps with `floor_char_boundary`.
- Slice with `.get(a..b)`, never `&s[a..b]`, on any range that did not come straight from the same tree.
- State the line break (tree-sitter rows count `\n` only, so a CR stays on the line) and the BOM policy.
- tree-sitter skips a leading BOM as whitespace, but byte offsets and the row-0 `Point.column` still count its 3 bytes. Editors and LSP hide the BOM, so subtract it on row 0 at the output boundary.
- Test every conversion with `scripts/adversarial-fixtures.mjs` (see `references/testing.md`).

Next: walking a tree → `references/walking.md`.
