# Parsing, generated contracts and source edits

Load for source parsers, structural queries, generated types or edits by source position. Inspect the selected parser/protocol version and the component that owns its limits.

## Contract ownership

- Identify the schema/type/codegen owner. Change the source declaration, regenerate with the established command and update its consumers; do not create handwritten copies of generated wire types.
- Verify that the built consumer actually embeds/loads the new contract. Stale generated output and a stale running server are different failure modes.
- Preserve documented extension fields and discriminators. Strict rejection follows a closed authored contract, not a preference for uniformity.
- Check field behavior, error representation and each affected interface, not just that regeneration succeeded. For Octocode's exact pipeline, use [repository guidance](octocode.md).

## Parser and query lifecycle

- Use full parsing for independent batch inputs unless incremental state has a demonstrated benefit. Incremental parsing needs a correctly edited tree and the same source identity.
- Reuse parsers/compiled queries only when their mutation, reset and thread-safety contracts permit it and measurement justifies retained state. Do not assume an FFI-backed type is shareable.
- Reject unsupported predicates or invalid query forms explicitly; silently ignoring them changes the selected evidence.
- Follow the engine's work/cancellation/resource policy. A guessed match cap must not make remaining matches unreachable. Report a terminal engine limit or provide a reachable continuation.
- Account for deeply nested inputs in parsing, traversal and destruction. Choose supported iterative or isolated execution strategies; a larger thread stack alone is not proof that arbitrary input is safe.

## Coordinates and rewrites

| Risk | Required check |
|---|---|
| Mixed units | State whether an offset is bytes, Unicode scalar values, UTF-16 units or display columns. Centralize conversion and use the connection's negotiated encoding. LSP defaults to UTF-16 when another encoding is not negotiated. |
| Invalid UTF-8 boundary | Validate ranges; `str::get` checks bounds and character boundaries. A byte slice checks bounds only. Reject invalid edit coordinates rather than rounding to a different edit. |
| Stale source | Bind edits to the exact source snapshot and recheck expected text/hash before writing. |
| Overlap and offset shifts | Reject overlapping edits; apply disjoint edits in descending start order, with a defined policy for insertions at the same offset. |
| Invalid output | Reparse staged output and compare relevant syntax diagnostics; clean splicing alone does not imply valid source. |
| Partial multi-file update | Use the established transaction/staging/rollback policy so a failed write does not leave an unexplained partial result. |

Test Unicode, non-BMP characters, combining characters, CRLF, trailing newlines, empty input, invalid bytes where accepted, stale snapshots and malformed/deep inputs. An absence or completeness claim requires every selected match/page to be reachable exactly once.

Sources: [LSP position encoding](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#position), [std str](https://doc.rust-lang.org/std/primitive.str.html), [tree-sitter basic parsing](https://tree-sitter.github.io/tree-sitter/using-parsers/2-basic-parsing.html).
