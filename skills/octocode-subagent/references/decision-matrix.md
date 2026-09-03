# Decision Matrix

Load when unsure whether to offload to the local Ollama worker. Why: offload that creates more verify work than it saves is a loss.

Core question: does offloading **free orchestrator context/budget** without creating more verify work than it saves? If no → stay solo.

## Matrix

| Complexity | Volume / tokens | Local tools needed? | Action |
|---|---|---|---|
| High (design, security, contested) | Any | — | Orchestrator only |
| Medium (multi-file edit with judgment) | Any | — | Orchestrator; optional local draft of *notes* only |
| Low (summarize, extract, label, draft, translate) | Large | No | **Offload** |
| Low | Small | No | **Offload OK** — prefer warm small/balanced; solo if local cold/slower and user did not ask local |
| Any | Any | Yes (MCP, shell agent loop) | Orchestrator or host subagent — **not** Ollama worker |

## Complexity signals

| Signal | Keep on orchestrator (High) | Local candidate (Low) |
|---|---|---|
| Subject | Architecture / API contracts / auth / tokens; security or privacy-sensitive reasoning | Per-file summaries; field extraction into a fixed schema |
| Shape | Root-cause across unknown surfaces; final merge of conflicting evidence | Labeling tickets/logs with a closed label set; first-pass boilerplate that the orchestrator edits and tests |

## Offload ROI check

Offload only if **all** are true:

1. Input is large enough that loading it all into the orchestrator hurts (rough guide: many files or long logs).
2. Output schema is tight enough to verify mechanically.
3. Wrong local answers are cheap to detect (path checks, schema, spot-read).
4. Latency of local + verify is acceptable to you.

## Prefer non-LLM first

Before any local model call, prefer `rg` / tests / formatters / typecheckers, existing scripts in the repository, or a host "small/fast" cloud model through normal subagent routing when already configured. Local Ollama is for when those are unavailable or the corpus is too large for the orchestrator window.

Next: offload = yes → **ROUTE** with `references/model-selection.md` using **live** `ollama list` and size/capability tiers (named tags in other refs are examples only); surface still unclear → `references/usage-matrix.md`.
