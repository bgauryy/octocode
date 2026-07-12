# Research Algorithm

Read first for routing, proof grades, triangulation, and failure recovery. Route from the strongest handle already known; never force a fixed grep→AST→LSP pipeline.

## Router
| Handle | First move |
|---|---|
| none + docs/wiki | extract named entry points, then verify exact claims |
| none | tree depth 1-2 + count matches per file; re-enter at hotspots |
| concept/behavior | synonym regex → symbols view for anchors |
| identifier | workspace symbol → callers/callees/references by symbol kind |
| code shape | structural rule with metavariables |
| installed package | inspect `node_modules` exact version before GitHub |
| why/history | PR/commit history on the path |

## Proof Model
Read at least two dimensions before a nontrivial conclusion:

| Dimension | Proves | Blind spot |
|---|---|---|
| structure | location, size, layout | behavior |
| stream | exact text/slices/symbols | symbol identity |
| connections | references/callers/AST shape | dynamic/unsupported paths |

Evidence grades: semantic (LSP identity), structural (AST shape), lexical (coverage, not identity), provider (weakest; index-limited). Before “unused/only/safe/impact,” diff package-wide text hits against LSP and include tests/scripts/configs.

## Execution Rules
- Batch independent probes; claims get 2-3 angles on the same target.
- Prefer `matchString` anchors, then line ranges; use full exact content only for small files.
- Quote/edit only exact content. Symbols orient; standard/minified output may rewrite text.
- Materialize a remote area before AST/LSP, exact absence, repeated many-file reads, or a third deep read.
- Read the tool/schema contract immediately before raw/OQL calls; follow returned cursors and `next.*`.
- For `node_modules`, disable default exclusions and inspect the file the resolver actually loads.

## Failure Signals
| Signal | Meaning → move |
|---|---|
| empty + search stats | negative only for that scope → change scope/synonym/filter once |
| typed error/hint | failure, not absence → follow the hint |
| structural zero | likely incomplete pattern → widen node shape or use a rule |
| LSP unavailable/incomplete | capability/truncation → exact/AST/text fallback |
| GitHub empty/unindexed | provider blind spot → verify path, materialize, search locally |
| resolved ref differs | default-branch fallback → cite/recheck actual ref |
| warning/redaction/pagination | interpretation changed → preserve and follow it |

Avoid guessed offsets/fields, serial single queries, remote over-reading, snippet conclusions, and claims from one evidence lane. No embeddings/index are assumed; for conceptual queries use tree → hotspot map → symbols → exact proof.
