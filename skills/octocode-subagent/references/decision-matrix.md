# Decision Matrix

Load when unsure whether to offload to the local Ollama worker.

## Core question

Does offloading **free orchestrator context/budget** without creating more verify work than it saves?

If no → stay solo.

## Matrix

| Complexity | Volume / tokens | Local tools needed? | Action |
|---|---|---|---|
| High (design, security, contested) | Any | — | Orchestrator only |
| Medium (multi-file edit with judgment) | Any | — | Orchestrator; optional local draft of *notes* only |
| Low (summarize, extract, label, draft, translate) | Large | No | **Offload** |
| Low | Small | No | **Offload OK** — prefer warm small/balanced; solo if local cold/slower and user did not ask local |
| Any | Any | Yes (MCP, shell agent loop) | Orchestrator or host subagent — **not** Ollama worker |

## Complexity signals

**High — keep on orchestrator**

- Architecture / API contracts / auth / tokens
- Root-cause across unknown surfaces
- Security or privacy-sensitive reasoning
- Final merge of conflicting evidence

**Low — local candidate**

- Per-file summaries
- Field extraction into a fixed schema
- Labeling tickets/logs with a closed label set
- First-pass boilerplate the orchestrator will edit and test

## Offload ROI check

Offload only if **all** are true:

1. Input is large enough that loading it all into the orchestrator hurts (rough guide: many files or long logs).
2. Output schema is tight enough to verify mechanically.
3. Wrong local answers are cheap to detect (path checks, schema, spot-read).
4. Latency of local + verify is acceptable to the user.

## Prefer non-LLM first

Before any local model call, prefer:

- `rg` / tests / formatters / typecheckers
- Existing scripts in the repo
- Host “small/fast” cloud model via normal subagent routing when already configured

Local Ollama is for when those are unavailable or the corpus is too large for the orchestrator window.

## After offload = yes

Continue to **ROUTE** / model select (`references/model-selection.md`) using **live** `ollama list` and size/capability tiers.  
Named tags in other refs are examples only.
