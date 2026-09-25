# Primitives: requests, gates, result shapes

Load when adding or changing an LSP operation, or when normalizing its results. Why: every primitive has several legal result shapes and a capability gate. A missed shape silently drops results, and a missed gate hangs or errors.

| Operation | Method | Gate | Normalize |
|---|---|---|---|
| definition / declaration / typeDefinition / implementation | `textDocument/*` | `*Provider` | `Location \| Location[] \| LocationLink[] \| null`. For a link, identity = `targetSelectionRange` and context = `targetRange`. |
| references | `textDocument/references` | `referencesProvider` | `Location[] \| null`. `context.includeDeclaration` is **required**. |
| hover | `textDocument/hover` | `hoverProvider` | `MarkupContent \| MarkedString \| MarkedString[] \| null` |
| documentSymbol | `textDocument/documentSymbol` | `documentSymbolProvider` | nested `DocumentSymbol[]` (only if declared) **or** flat `SymbolInformation[]` with `containerName` |
| workspace/symbol | `workspace/symbol` | `workspaceSymbolProvider` | `SymbolInformation[] \| WorkspaceSymbol[]`. The range may be absent and need `workspaceSymbol/resolve`. |
| call hierarchy | `prepareCallHierarchy` → `callHierarchy/incomingCalls\|outgoingCalls` | `callHierarchyProvider` | `{from\|to, fromRanges[]}`. `fromRanges` are the call sites, in the **caller's** file. |
| type hierarchy | `prepareTypeHierarchy` → `typeHierarchy/supertypes\|subtypes` | `typeHierarchyProvider` (absent on TS and pylsp) | `TypeHierarchyItem[]` |
| diagnostics (pull) | `textDocument/diagnostic` | `diagnosticProvider` | `full \| unchanged` (with `resultId`). ServerCancelled with `retriggerRequest` → retry. |
| diagnostics (push) | `textDocument/publishDiagnostics` | (always possible) | No "done" signal. Wait for a version ≥ the version you opened, within a bounded quiet window. |

## Rules
- **Gate before sending.** A missing provider becomes a typed `unsupportedOperation`/`capabilityUnavailable` row. Check dynamic registrations too, if you enable them.
- **`null` means none, not an error.** An empty result means "none within this server's scope". It is never a proof of global absence.
- **Round-trip hierarchy items verbatim**, including `data`. rust-analyzer and tsserver need it back byte for byte. Never rebuild an item from its name and range.
- **Open before you query.** Many servers, tsserver especially, answer only for opened documents and return "No Project" or empty otherwise. Every hop target you query also needs opening, and that costs document slots.
- **Partial results:** send a `partialResultToken` on large operations and merge the `$/progress` partials under a size cap. Decide explicitly whether going over the cap **fails** the request (octocode does) or truncates with a flag.
- **Prefer pull diagnostics** when advertised. With push, an empty result means "not published", not "clean".
- **Cap every list before serializing.** Sort deterministically (uri, start, end) and dedupe by `(canonical uri, range)`.
- **`workspace/symbol` is fuzzy and server-ranked.** Use it with a specific query, cap the results, and filter exact or suffix matches (`X`, `.X`, `::X`) yourself.

Next: for multi-hop traversal over these primitives load `references/walking.md`.
