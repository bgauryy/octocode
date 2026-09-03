---
name: octocode-subagent
description: "Use when orchestrating or delegating substantial work: choose solo, batch, subagents, specialist handoffs, or A2A peers; coordinate independent workstreams through measurable completion; challenge claims with fresh reviewers; or offload sealed one-shots to local Ollama. Skip routine edits, explanations, dependent steps, and cheap batchable reads."
---
# Octocode Orchestration
Accountable, host-independent orchestration: the parent owns user intent, authority, integration, evidence, and verdict; workers supply bounded results, never authority.
Flow: `FRAME → GATE → DECOMPOSE → ROUTE → PACKET → SPAWN/HANDOFF → COORDINATE → VERIFY → SYNTHESIZE → CLEANUP → REPORT` (tool-using) · `GATE → ROUTE → RUN → VERIFY → REPORT` (Ollama).
Workspace output contract: chat-only synthesis stays in chat. Worker packets and generated results default to `<workspace>/.octocode/worker/`; transient prompts use `<workspace>/.octocode/tmp/ollama-worker/`. User-approved source edits keep their named paths. Never fall back to a user-level Octocode home for artifacts.
## Lobby rules
1. Frame substantial work before fan-out; never broaden intent, permissions, effects, deletion scope, or budget because this skill activated.
2. Spawn only when delegation changes speed, expertise, isolation, or context quality; default solo and batch known independent reads.
3. One bounded objective per worker; no nested spawning unless the host explicitly allows it and a new value/cost gate passes.
4. Workers inherit no parent chat: packets carry goal, scope, context, authority, budget, evidence, acceptance, ownership, and return shape.
5. Treat worker output as claims; re-check load-bearing anchors in the parent (Ollama: always VERIFY).
6. Reach the worker barrier before synthesis; keep `partial`, `blocked`, conflicts, and dissent visible.
7. Parent owns you, integration, irreversible actions, and final verdict unless an explicit handoff transfers user contact within the same authority ceiling.
8. Pick the smallest capable configured model; challenge techniques use fresh context and agreement is not proof; local Ollama is tool-less one-shot/map-reduce only.
Stop when solo work finishes, two High options need a winner, three angles add nothing, a user/auth gate is pending, or no live workers remain.
## Smart routes — load only what the current step needs
- At FRAME, load `references/orchestration-contract.md` when goal, authority, budget, ownership, or critical path needs definition — bound activity to you-visible outcome.
- When deciding solo, batch, specialist, or clean worker, load `references/spawn-gate.md` — delegation must earn its coordination cost.
- When splitting work, load `references/decompose.md`; when choosing supervisor, pipeline, handoff, or swarm load `references/patterns.md` — create a dependency-aware topology.
- Before spawning, load `references/packets.md`; when delegating technical research load `references/octocode.md` — make worker context and tool routing self-contained.
- When selecting host model/thinking effort, load `references/model-routing.md` — smallest capable configured model.
- When waiting, steering, messaging, or stopping workers, load `references/coordinate.md`; for independent remote peers load `references/a2a.md`.
- When parallel writers share mutable state, load `references/workspace.md`; when peers, locks, messages, verification debt, or reusable memory can change EXECUTE/VERIFY, load `references/awareness.md`.
- For behavior changes use red→green TDD; when improvement needs a KPI, held-out cases, or strategy comparison load `references/evaluation.md` — freeze the sensor before mutation.
- When workers stall, fail, or conflict, load `references/recovery.md`; before final output load `references/synthesize.md` and `references/output.md`.
- At CLEANUP/REPORT load `references/completion.md` — recheck integrated anchors, documentation, authorized cleanup, and real host/CLI behavior.
- When grounding orchestration guidance in sources, load `references/references.md`.
- When improving this skill, prefer `octocode-eval-benchmark`; otherwise load `references/improve-loop.md`.
## Challenge routes — fresh context per critic; agreement is not proof
- When quality risk needs a second mind, load `references/techniques.md` first — it names which technique below earns the spawn.
- When a plan needs cheap assumption surfacing without new research, load `references/rubber-duck.md`; when another agent’s claims need claim-by-claim falsification, load `references/interview.md`.
- When a worker must follow a borrowed playbook without borrowed chat, load `references/mimic-flow.md`; when a design looks too clean to ship, load `references/red-team.md`.
- When a critic must judge the artifact and not the author’s story, load `references/blind-review.md`; when one solve stays ambiguous and independent retries can cut noise, load `references/consensus.md`.
## Local Ollama routes — tool-less one-shot / map-reduce offload only
- When saving tokens with local Ollama (summarize/extract/…), load `references/local-ollama.md` — not a Task/A2A spawn path.
- When running that offload loop end to end, load `references/workflow.md` — health GATE, ROUTE, RUN shards, VERIFY, REPORT what was offloaded. <!-- style-lint: ignore-line passive-voice -->
- When unsure whether offload beats solo, load `references/decision-matrix.md`; when the surface is unclear (research, article, code, translate, images), load `references/usage-matrix.md`.
- When selecting Ollama tags, load `references/model-selection.md`; when an installed family needs special flags or two families tie, load `references/family-playbooks.md`.
- When writing the sealed packet, load `references/packet-contract.md`; for the example JSON schemas it references, load `references/packet-schemas.md`.
- When inventorying models or debugging CLI behavior, load `references/ollama-cli.md`; for `ollama run` flags, non-interactive patterns, and HTTP equivalents load `references/ollama-cli-run.md`; for script invoke and serving knobs load `references/ollama-invoke.md`.
- Before integrating any worker output, load `references/verify-gate.md` — pass, one tighter packet, one cascade, or solo; never silent-accept.
- When the question is RAM kits, catalog, or MCP/tools capability rather than routing, load `references/ollama-local-models.md` — pull commands per RAM in `references/ollama-local-models-kits.md`, capability rows in `references/ollama-local-models-matrix.md`, cloud/heavy tags plus a sample inventory in `references/ollama-local-models-heavy.md`, evidence and links in `references/ollama-local-models-sources.md`.
- Use `octocode-research` for worker evidence; `octocode-eval-benchmark` for worker quality; `octocode-rfc-generator` before multi-agent architecture changes; `octocode-prompt-optimizer` for packet contracts; `octocode-skills` for this folder.
## Scripts
- Run `scripts/ollama-health.sh` at GATE and after model ROUTE; run `scripts/ollama-worker.sh` once per sealed packet or shard at RUN with `--job`, `--input`, `--schema`, `--out`, and `--keepalive`.
- After changing tool-using orchestration, run `scripts/eval-contract.mjs`; it validates `evals/cases.json`, while `--results` grades only a fresh current-digest receipt kept outside the shipped skill.
