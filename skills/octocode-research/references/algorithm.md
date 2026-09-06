# Research Algorithm

Load when any research run starts and you need routing, proof grades, triangulation, or failure recovery. Why: the strongest handle you already hold decides the first move — never force a fixed grep→AST→LSP pipeline.

## Router
| Handle | First move |
|---|---|
| none + docs/wiki | extract named entry points, then verify exact claims |
| none | tree depth 1-2 + count matches per file; re-enter at hotspots |
| concept/behavior | synonym regex → symbols view for anchors |
| identifier | text or workspaceSymbol for a real location, then LSP if identity matters |
| code shape | structural rule with metavariables |
| file/repository topology | `localAnalyzeGraph`: dependencies/dependents/path/cycles/reachability |
| installed package | inspect resolved version/source when access permits; compare the matching upstream release |
| why/history | PR/commit history on the path |

## Proof Model
Choose evidence for the claim; cross-check consequential conclusions:

| Dimension | Proves | Blind spot |
|---|---|---|
| structure | location, size, layout | behavior |
| stream | exact text/slices/symbols | symbol identity |
| connections | graph paths/SCCs/reachability + LSP references/callers/AST shape | dynamic/unsupported paths |

Evidence kinds are not a universal ranking: LSP answers identity, AST answers syntax, exact reads answer text, and providers answer their declared data scope. Before “unused/only/safe/impact,” compare relevant text hits with LSP and account for tests/scripts/configs and runtime registrations.

## Execution Rules
- Batch independent probes; add another lane when it can disconfirm the claim.
- Prefer `matchString` anchors, then line ranges; use full exact content only for small files.
- Quote/edit only exact content. Symbols orient; standard/minified output might rewrite text.
- Let `references/workflow-combination.md` decide materialization from evidence needs and scope.
- Read the tool/schema contract immediately before raw calls; graph compact schemas might flatten operation variants, so use full JSON when `file`/`target` requirements are unclear.
- Honor repository access restrictions before inspecting vendor/generated files; record any unresolved version gap.

## Failure Signals
| Signal | Meaning → move |
|---|---|
| empty + search stats | negative only for that scope → change scope/synonym/filter once |
| typed error/hint | failure, not absence → follow the hint |
| structural zero | inspect completion/diagnostics first; only a complete search earns pattern revision |
| LSP unavailable/incomplete | capability/truncation → exact/AST/text fallback |
| GitHub empty/unindexed | provider blind spot → verify path, materialize, search locally |
| resolved ref differs | scope changed → verify the reported ref against the intended target |
| warning/redaction/pagination | interpretation changed → preserve and follow it |

Avoid guessed offsets/fields, unnecessary rereads, and snippet-based behavior claims. Serial calls are appropriate when one result determines the next. Conceptual queries can use synonyms, tree, or symbols to find a decisive exact anchor.

Next: classify the request with `references/problem-framing.md`, then take one route from `references/workflows.md`; when a code claim needs the proof ladder load `references/code-research.md`; when a call itself fails or a surface is gated load `references/octocode.md`. <!-- style-lint: ignore-line passive-voice -->
