---
name: octocode-subagent
description: "Use when spawning workers or offloading work: Task/subagents, specialist handoffs, A2A peers, sealed packets, coordinate/synthesize, challenge techniques (rubber-duck, interview, mimic-flow, red-team, blind review, consensus), or local Ollama one-shots to save tokens. Measuring keep/discard KPIs → octocode-graph-eval."
---

# Octocode Subagent

Host-agnostic delegation: cloud/host workers **or** local Ollama offload.  
Flows: `GATE → DECOMPOSE → ROUTE → PACKET → SPAWN → COORDINATE → SYNTHESIZE → CLEANUP` (tool-using) · `GATE → ROUTE → RUN → VERIFY → REPORT` (Ollama — `references/local-ollama.md`).

## Lobby rules
1. Spawn only when delegation changes speed, expertise, isolation, or context quality; otherwise keep work in the parent. Default is solo; earn spawn cost first.
2. One bounded objective per worker; no nested spawning unless the host explicitly allows it.
3. Workers inherit no parent chat: every packet carries goal, scope, context, authority, constraints, evidence needs, and return shape.
4. Treat worker output as claims; re-check load-bearing anchors (Ollama: always VERIFY).
5. Barrier before synthesize — wait/list every live worker (or stop+remove); merge conflicts first; then answer.
6. Parent owns the user, synthesis, and mutations unless a packet explicitly transfers write ownership.
7. Pick the smallest capable configured model; declare file ownership before parallel writes.
8. Challenge techniques use **fresh context**; agreement is not proof.
9. Local Ollama is tool-less one-shot/map-reduce only — never a tool-using agent loop.
Stop when solo work finishes, two High options need a winner, three angles add nothing, a user/auth gate is pending, or no live workers remain.

## Smart routes — load only what the current step needs
- When deciding solo, batch, specialist, or clean worker, load `references/spawn-gate.md` — delegation must earn its coordination cost.
- When saving tokens with local Ollama (summarize/extract/…), load `references/local-ollama.md` — not a Task/A2A spawn path.
- When splitting work, load `references/decompose.md`; when choosing supervisor, pipeline, handoff, or swarm load `references/patterns.md` — create a dependency-aware topology.
- When quality risk needs a second mind, load `references/techniques.md` first — then the matching technique ref.
- Before spawning, load `references/packets.md`; when delegating technical research load `references/octocode.md` — make worker context and tool routing self-contained.
- When selecting host model/thinking effort, load `references/model-routing.md`; when selecting Ollama tags load `references/model-selection.md`.
- When waiting, steering, messaging, or stopping workers, load `references/coordinate.md`; for independent remote peers load `references/a2a.md`.
- When parallel writers share mutable state, load `references/workspace.md`.
- When workers stall, fail, or conflict, load `references/recovery.md`; before final output load `references/synthesize.md` and `references/output.md`.
- When grounding orchestration guidance in sources, load `references/references.md`.
- When improving this skill, prefer `octocode-graph-eval`; otherwise load `references/improve-loop.md`.

## Related routes
- Use `octocode-research` for worker evidence; `octocode-graph-eval` to judge worker quality (subagent measurement cookbooks live there).
- Use `octocode-rfc-generator` before changing a multi-agent architecture; `octocode-prompt-optimizer` for packet contracts; `octocode-skills` when changing this folder.

## Scripts
- `scripts/eval-subagent.mjs` — spawn/technique smoke (`--self-test` / `--triggers` / `--case`)
- `scripts/ollama-health.sh` · `scripts/ollama-worker.sh` · `scripts/eval-ollama.mjs` — local offload path (`evals/ollama/`)
