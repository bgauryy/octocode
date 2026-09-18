---
name: octocode-subagent
description: "Use when choosing execution before spawning agents: solo/batch/subagent/local-Ollama, decomposition, and handoffs."
---
# Octocode Orchestration
tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.
The parent owns user intent, authority, integration, evidence, and verdict; workers return bounded results, never authority. Packets/results: `<output>/worker/`; transient prompts: `<output>/tmp/ollama-worker/`. Chat-only synthesis stays in chat; approved source edits keep their paths.
Flow: `FRAME → GATE → DECOMPOSE → ROUTE → PACKET → SPAWN/HANDOFF → COORDINATE → VERIFY → SYNTHESIZE → CLEANUP → REPORT` (tool-using) · `GATE → ROUTE → RUN → VERIFY → REPORT` (Ollama).
## Lobby rules
1. Frame substantial work before fan-out; never broaden intent, permissions, effects, deletion scope, or budget because this skill activated.
2. Spawn only when delegation changes speed, expertise, isolation, or context quality; default solo and batch known independent reads.
3. One bounded objective per worker; no nested spawning unless the host explicitly allows it and a new value/cost gate passes.
4. Check worker context. Add only missing goals, scope, evidence, authority, ownership, and acceptance.
5. Treat worker output as claims; re-check load-bearing anchors in the parent (Ollama: always VERIFY).
6. Reach the worker barrier before synthesis; keep `partial`, `blocked`, conflicts, and dissent visible.
7. Parent owns user communication, integration, irreversible actions, and the final verdict unless an explicit handoff transfers contact within the same authority ceiling.
8. Respect the requested model or host default; otherwise select a capable configured model for the work. Challenge techniques use fresh context, and agreement is not proof; local Ollama is tool-less one-shot/map-reduce only.
Stop when acceptance is met or progress needs missing authority or information. Completed workers trigger parent verification and integration; an empty worker list does not mean the task is done.
## Smart routes — load only what the current step needs
- At FRAME, load `references/orchestration-contract.md` when goal, authority, budget, ownership, or critical path needs definition; when deciding solo, batch, specialist, or clean worker, load `references/spawn-gate.md`.
- When splitting work, load `references/decompose.md`; when choosing supervisor, pipeline, handoff, or swarm, load `references/patterns.md`. Before spawning, load `references/packets.md`; when delegating technical research, use `octocode-research`.
- When selecting host model/thinking effort, load `references/model-routing.md`; when waiting, steering, messaging, or stopping workers, load `references/coordinate.md`; for independent remote peers, load `references/a2a.md`.
- When parallel writers share mutable state, load `references/workspace.md`; when peers, locks, messages, verification debt, or reusable memory can change EXECUTE/VERIFY, load `references/awareness.md`.
- For behavior changes use red→green TDD; when improvement needs a KPI, held-out cases, or strategy comparison load `references/evaluation.md` — freeze the sensor before mutation.
- When workers stall, fail, or conflict, load `references/recovery.md`; before final output load `references/synthesize.md` and `references/output.md`.
- At CLEANUP/REPORT load `references/completion.md` — recheck integrated anchors, docs, authorized cleanup, and real host/CLI behavior.
- To audit orchestration sources, load `references/references.md`.
- When improving this skill, use `octocode-eval-benchmark`.
## Challenge routes — fresh context per critic; agreement is not proof
- When quality risk needs a second mind, load `references/techniques.md` first — it names which technique below earns the spawn.
- When a plan needs cheap assumption surfacing without new research, load `references/rubber-duck.md`; when another agent’s claims need claim-by-claim falsification, load `references/interview.md`.
- When a worker must follow a borrowed playbook without borrowed chat, load `references/mimic-flow.md`; when a design looks too clean to ship, load `references/red-team.md`.
- When a critic must judge the artifact and not the author’s story, load `references/blind-review.md`; when one solve stays ambiguous and independent retries can cut noise, load `references/consensus.md`.
## Local Ollama routes — tool-less one-shot / map-reduce offload only
- When saving tokens with local Ollama (summarize/extract/…), load `references/local-ollama.md` — not a Task/A2A spawn path.
- When running that offload loop end-to-end, load `references/workflow.md` — health GATE, ROUTE, RUN shards, VERIFY, REPORT the offloaded work.
- When unsure whether offload beats solo, load `references/decision-matrix.md`; when the surface is unclear (research, article, code, translate, images), load `references/usage-matrix.md`.
- When selecting Ollama tags, load `references/model-selection.md`; when an installed family needs special flags or two families tie, load `references/family-playbooks.md`.
- When writing the sealed packet, load `references/packet-contract.md`; for the example JSON schemas it references, load `references/packet-schemas.md`.
- When inventorying models or debugging CLI behavior, load `references/ollama-cli.md`; for `ollama run` flags, non-interactive patterns, and HTTP equivalents load `references/ollama-cli-run.md`; for script invoke and serving knobs load `references/ollama-invoke.md`.
- Before integrating worker output, load `references/verify-gate.md` — pass, one tighter packet, one cascade, or solo; never silent-accept.
- When the question is RAM kits, catalog, or MCP/tools capability rather than routing, load `references/ollama-local-models.md` — pull commands per RAM in `references/ollama-local-models-kits.md`, capability rows in `references/ollama-local-models-matrix.md`, cloud/heavy tags plus a sample inventory in `references/ollama-local-models-heavy.md`, evidence and links in `references/ollama-local-models-sources.md`.
- Use `octocode-research` for worker evidence; `octocode-eval-benchmark` for worker quality; `octocode-rfc-generator` before multi-agent architecture changes; `octocode-prompt-optimizer` for packet contracts; `octocode-skills` for this folder.
## Scripts
- Run `scripts/ollama-health.sh` at GATE and after model ROUTE; run `scripts/ollama-worker.sh` once per sealed packet or shard at RUN with `--job`, `--input`, `--schema`, `--out`, and `--keepalive`.
- After changing tool-using orchestration, run `scripts/eval-contract.mjs`; it validates `evals/cases.json`, while `--results` grades only a fresh current-digest receipt kept outside the shipped skill.
