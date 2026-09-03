# Local Ollama
Load when saving tokens with a **local Ollama** worker while the cloud/parent agent keeps tools and judgment. Why: tool-less one-shot/map-reduce offload is a different substrate from Task/A2A spawn (`spawn-gate.md`).

**Delegate execution, retain reasoning.** Parent keeps tools/fetch/verify/writes. Local Ollama does sealed-packet token burn on text/images you already have.

**Portable:** pick from live `ollama list` + size/capability tiers (named tags are examples only).

Flow: `GATE → ROUTE → RUN → VERIFY → REPORT` — full steps in `references/workflow.md`.  
VERIFY = quality gate before accept or cascade (small → stronger installed → solo).

**Routine loads:** `model-selection.md` (ROUTE) + `verify-gate.md` (VERIFY). Surfaces unclear → `usage-matrix.md`.

## Hard rules
1. Architecture, security, design, final synthesis, and repository writes stay on the parent unless you transfer write ownership.
2. Worker output is **untrusted** — VERIFY before integrate (`references/verify-gate.md`).
3. Prefer deterministic scripts over any LLM when they suffice.
4. No tool-using agent loops on Ollama (single-shot / map-reduce only).
5. Health-check before first invoke; if down, stay solo (`scripts/ollama-health.sh`).
6. Use exact names from `ollama list` — never invent tags; skip embed/OCR-only for chat jobs.
7. Worker never browses the web — parent fetches; worker sees saved text/images only.
8. No image generation / inventing images on the worker — vision caption of provided images only.

Jobs (not an exhaustive list — similar low-risk work is valid): summarize | extract | classify | draft | map | checklist/check | vision | translate.

## When
Token/context pressure; low-complexity summarize/extract/classify/translate/draft/check/vision/article-after-fetch; user asks for local/Ollama/save-tokens; warm small one-shots / Offload OK for save-tokens.

**Catalog shortcut:** RAM kit / capability Q only → `ollama-local-models.md`, skip full offload.

## When NOT / Do not use this path for
High-complexity, security, live tools/MCP/browser loops, contested multi-source synthesis, no fitting chat model. Parallel **tool-using** cloud workers → `references/spawn-gate.md` / patterns — not this path.

## Scripts
- `scripts/ollama-health.sh` — GATE
- `scripts/ollama-worker.sh` — RUN sealed packet

## Progressive refs
| Need | Ref |
|---|---|
| Full loop | `references/workflow.md` |
| Offload vs solo | `references/decision-matrix.md` / `references/usage-matrix.md` |
| Pick model | `references/model-selection.md` |
| Family flags | `references/family-playbooks.md` |
| Verify | `references/verify-gate.md` |
| Packet schema | `references/packet-contract.md` |
| CLI / HTTP | `references/ollama-cli.md` · `references/ollama-invoke.md` |
| Catalog/RAM only | `references/ollama-local-models.md` |

Next: run health → `references/workflow.md`; spawn tool-using workers → `references/spawn-gate.md`.
