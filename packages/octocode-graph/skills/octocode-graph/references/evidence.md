# Interpreting graph evidence

A native witness gives the source of a relationship. For example, an import at a particular line establishes a dependency. Use a call site and an LSP definition or caller when a claim depends on a specific overload, implementation or receiver.

In a saved graph, `via: graph` identifies static linkage, `via: lsp` a call the language server found, and `via: both` agreement. A cross-chunk name match can be ambiguous. For `locationsWithheld`, a returned definition can confirm its own edge while the overall probe stays incomplete. The final `callStats.unanswered` records each incomplete probe and its reasons.

When a query returns zero rows, check the snapshot scope, freshness, confidence filter and unresolved sites before concluding a symbol has no consumers. Dynamic dispatch, configuration and registration can need a source read or runtime check. For a count, the response and row continuations identify the remaining evidence. [Native results](build-and-read.md) · [Viewer coverage](commands.md)

Bulk exports save the raw cursor replies. The viewer streams each completed graph slice, joining all parts of a split edge first. A cursor failure, conflicting snapshot, or inconsistent exact total invalidates the walk.
