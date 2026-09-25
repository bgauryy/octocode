# Agent Usage: lspSearch and LSP tools for agents

Load when a research or refactor step needs semantic identity through `lspSearch` (CLI `$OCTO lspSearch '<json>'` or the MCP tool), or when designing an agent-facing LSP tool surface. Why: the answer can only be as precise as the anchor. Most bad LSP answers come from a guessed anchor, mixed-up line numbering, a walk read as direct edges, or an empty result read as absence.

## When LSP beats grep, and when it loses
A 2026 ablation over Claude models (arXiv 2608.13568) found:
- LSP **costs** 6–118% more tokens when locating a symbol by name.
- LSP **adds precision** on reference-completeness tasks.
- A location-only LSP rename **fails most multi-file renames**, because it misses strings and comments.

Cursor also reports that semantic search combined with grep beats either one alone. **Route by task:**

| Task | First tool |
|---|---|
| Find where X is defined or named | `localSearch` / `astSearch` |
| Who calls or uses this exact symbol | `lspSearch` references or callers |
| Is it safe to rename or delete | LSP references **plus** exact text search (strings, comments, configs) |
| What type is this, or what implements it | `lspSearch` hover, typeDefinition, implementation |

## The cheap-first ladder
1. **Anchor from bytes.** Find the symbol with `localSearch`, `astSearch`, or a `documentSymbols` outline. Copy the observed 1-based line into `lineHint`, and never guess it.
2. **Batch on one anchor:** up to 5 queries (definition + references + callers) in one call. The server is warm after the first.
3. `references` with `includeDeclaration:false`. Add `groupByFile:true` for fan-out, or `contextLines:0-2` for rows you can rank.
4. `callers` or `callees` with **`depth:1`**, then walk hop by hop from the nodes you choose.
5. `diagnostic` after edits, and on the referrers when a signature changes (the blast radius).

## Line numbers (check before copying)
| Field | Counts from |
|---|---|
| `lineHint` (input) | **1** |
| `position` (input) | **0** (and UTF-16 columns) |
| Every output line and character (`displayRange`, `fromRanges`, `via`, `resolvedSymbol.foundAt*`, documentSymbols `line`/`character`) | 1 (UTF-16 columns) |

To reuse an output point as `position`, subtract 1 from line and character.

## Reading results honestly
- **Cold start:** the first rust-analyzer call on a large repo takes tens of seconds; warm calls take seconds. `readiness:"timeout"` or `languageServerIndexing` means **unknown**, so re-run once warm.
- `settledWithoutProgress` is normal for typescript-language-server.
- **`depth>1` is a BFS edge list:** every item has `level`, and `via` (the parent) below level 1; repeated callers merge into one edge. The node cap ends the walk with `next.continueWalk`; the per-node fan-out cap sets `terminalLimit`.
- **Empty is scoped.** `coverage.exhaustive:false` or an empty result proves only "none in this server's indexed scope". Before a delete or "unused" claim, cross-check with exact `localSearch` and `astSearch` topology.
- `rustContext` (features, target, cfgs) starts a **separate server**, which means another cold start and a different pagination snapshot. `buildScripts`/`procMacros` execute workspace code.
- Pagination: copy `next.nextPage` verbatim. When the snapshot changes, follow `next.restart` and throw away the earlier pages.
- `lsp.serverUnavailable`: install or point to a server (`octocode lsp-server status|install`, `OCTOCODE_*_SERVER_PATH`). Never pass off a syntactic guess as a semantic answer.

## Designing an agent-facing LSP surface
- **Anchor by name path plus a file scope** (Serena's `MyClass/method` + `relative_path`), with a line hint only as a tiebreaker. A raw `(line, column)` is the most brittle anchor an agent can be given.
- **Shape the output for tokens:** relative paths, grouping by file, the *enclosing symbol* for each reference, ±1-line snippets, bodies only on request, and a size cap that degrades step by step (full → no code → per-file counts → total).
- **Return an honest status**, `ready | indexing | unsupported | empty`, and **never cache empty results** (Serena). Claude Code issue #44767 shows the failure mode: gopls's "Loading packages…" was reported as "No definition found".

Next: for why results look this way load `references/primitives.md` and `references/walking.md`.
