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
- When splitting work, load `references/decompose.md`; when choosing supervisor, pipeline, handoff, or swarm load `references/patterns.md` — create a dependency-aware topology.
- Before spawning, load `references/packets.md`; when delegating technical research load `references/octocode.md` — make worker context and tool routing self-contained.
- When selecting host model/thinking effort, load `references/model-routing.md` — smallest capable configured model.
- When waiting, steering, messaging, or stopping workers, load `references/coordinate.md`; for independent remote peers load `references/a2a.md`.
- When parallel writers share mutable state, load `references/workspace.md`.
- When workers stall, fail, or conflict, load `references/recovery.md`; before final output load `references/synthesize.md` and `references/output.md`; at CLEANUP stop and remove every worker and release shared state per `references/coordinate.md`.
- When grounding orchestration guidance in sources, load `references/references.md`.
- When improving this skill, prefer `octocode-graph-eval`; otherwise load `references/improve-loop.md`.

## Challenge routes — fresh context per critic; agreement is not proof
- When quality risk needs a second mind, load `references/techniques.md` first — it names which technique below earns the spawn.
- When a plan needs cheap assumption surfacing without new research, load `references/rubber-duck.md`; when another agent’s claims need claim-by-claim falsification, load `references/interview.md`.
- When a worker must follow a borrowed playbook without borrowed chat, load `references/mimic-flow.md`; when a design looks too clean to ship, load `references/red-team.md`.
- When a critic must judge the artifact and not the author’s story, load `references/blind-review.md`; when one solve stays ambiguous and independent retries can cut noise, load `references/consensus.md`.

## Local Ollama routes — tool-less one-shot / map-reduce offload only
- When saving tokens with local Ollama (summarize/extract/…), load `references/local-ollama.md` — not a Task/A2A spawn path.
- When running that offload loop end to end, load `references/workflow.md` — health GATE, ROUTE, RUN shards, VERIFY, REPORT what was offloaded.
- When unsure whether offload beats solo, load `references/decision-matrix.md`; when the surface is unclear (research, article, code, translate, images), load `references/usage-matrix.md`.
- When selecting Ollama tags, load `references/model-selection.md`; when an installed family needs special flags or two families tie, load `references/family-playbooks.md`.
- When writing the sealed packet, load `references/packet-contract.md`; for the example JSON schemas it references, load `references/packet-schemas.md`.
- When inventorying models or debugging CLI behavior, load `references/ollama-cli.md`; for `ollama run` flags, non-interactive patterns, and HTTP equivalents load `references/ollama-cli-run.md`; for script invoke and serving knobs load `references/ollama-invoke.md`.
- Before integrating any worker output, load `references/verify-gate.md` — pass, one tighter packet, one cascade, or solo; never silent-accept.
- When the question is RAM kits, catalog, or MCP/tools capability rather than routing, load `references/ollama-local-models.md` — pull commands per RAM in `references/ollama-local-models-kits.md`, capability rows in `references/ollama-local-models-matrix.md`, cloud/heavy tags plus a sample inventory in `references/ollama-local-models-heavy.md`, evidence and links in `references/ollama-local-models-sources.md`.

## Related routes
- Use `octocode-research` for worker evidence; `octocode-graph-eval` to judge worker quality (subagent measurement cookbooks live there).
- Use `octocode-rfc-generator` before changing a multi-agent architecture; `octocode-prompt-optimizer` for packet contracts; `octocode-skills` when changing this folder.

## Scripts
- Run `scripts/ollama-health.sh` at GATE, then again with `--model "$OLLAMA_WORKER_MODEL"` after ROUTE — daemon or tag missing means stay solo.
- Run `scripts/ollama-worker.sh` once per sealed packet or shard at RUN — `--job`, `--input`, `--schema`, `--out`, `--keepalive`.
