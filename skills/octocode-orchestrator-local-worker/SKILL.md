---
name: octocode-orchestrator-local-worker
description: "Use when saving tokens with a local Ollama worker while the cloud agent keeps tools and judgment: summarize, extract, classify, translate, article digest after fetch, draft, checklist, vision caption, map-reduce, or phrases like ollama worker, local model, offload, use ollama."
---

# Orchestrator + Local Worker

**Delegate execution, retain reasoning.** Cloud agent keeps tools/fetch/verify/writes. Local Ollama does sealed-packet token burn on text/images you already have.

**Portable:** pick from live `ollama list` + size/capability tiers (named tags are examples only).

Flow: `GATE → ROUTE → RUN → VERIFY → REPORT` — full steps in `references/workflow.md`.  
VERIFY = quality gate before accept or cascade (small → stronger installed → solo).

**Routine loads:** `model-selection.md` (ROUTE) + `verify-gate.md` (VERIFY). Surfaces unclear → `usage-matrix.md`.

## Hard rules

1. Architecture, security, design, final synthesis, and repo writes stay on the orchestrator unless the user transfers write ownership.
2. Treat worker output as untrusted — never paste into answers/commits without VERIFY.
3. Prefer deterministic scripts over any LLM when they suffice.
4. No tool-using agent loops on the worker (single-shot / map-reduce only).
5. Health-check Ollama before first invoke; if down, stay solo.
6. Use exact names from `ollama list` — never invent tags or use embed/OCR-only models for chat jobs.
7. Prefer language **orchestrator / worker**.
8. Worker never browses the web — orchestrator fetches; worker sees saved text/images only.

## When to activate

Token/context pressure; low-complexity summarize/extract/classify/translate/draft/check/vision/article-after-fetch; user asks for local/Ollama/save-tokens; warm small one-shots.

**Catalog shortcut:** RAM kit / capability Q only → `ollama-local-models.md`, skip full offload.

## When NOT / not this skill

High-complexity, security, live tools/MCP/browser loops, contested multi-source synthesis, no fitting chat model, or host cloud is warmer and user did not ask local.  
Setup/pull → catalog. Tool-harness triage → triage skill. Parallel cloud workers → `octocode-subagent`. Full local agent bridges → out of scope.

## Scripts

- `scripts/ollama-health.sh` — GATE daemon/model check
- `scripts/ollama-worker.sh` — RUN sealed packet (`--keepalive`, `--temperature`, `--num-ctx`)
- `scripts/eval-skill.mjs` — suite runner (static/script/live)

## Progressive refs

| Ref | When |
|---|---|
| `references/workflow.md` | Full GATE→REPORT / recovery / job table |
| `references/usage-matrix.md` | Surface when/how |
| `references/decision-matrix.md` | Offload vs solo unclear |
| `references/model-selection.md` | Every ROUTE |
| `references/family-playbooks.md` | Family flags / tie-break |
| `references/ollama-local-models.md` | Catalog/RAM only — not routine |
| `references/ollama-cli.md` | CLI flags |
| `references/ollama-invoke.md` | HTTP / serving knobs |
| `references/packet-contract.md` | Packet schema |
| `references/verify-gate.md` | Every VERIFY |
| `references/references.md` | Provenance |

**Eval:** `evals/` permanent; `.octocode/` temp. Improving this skill → `octocode-eval`.
