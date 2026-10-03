---
name: octocode-subagent
description: "Use when substantial work has independent lanes that justify delegation cost: parallel specialist workers, local Ollama offload, or A2A handoffs. Not for: routine edits or dependent sequences where one batched call handles everything; explanations; known reads that fit a single call."
---
# Octocode Subagent
tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
The parent owns user intent, authority, user contact, integration, irreversible actions, evidence, and the final verdict, unless an explicit handoff transfers contact within the same authority ceiling. Workers return bounded claims, never authority. Write packets and results to `<output>/worker/` and transient prompts to `<output>/tmp/ollama-worker/`. Chat-only synthesis stays in chat; approved source edits keep their paths.
```mermaid
flowchart LR
  D{"DECIDE: delegation pays?"} -- no --> S["solo / batch in parent"]
  D -- yes --> K{"PICK worker kind"}
  K -- tool-using --> C["cloud subagent"]
  K -- "tool-less text" --> O["local Ollama"]
  K -- "remote peer" --> A["A2A peer"]
  C --> B["BRIEF: sealed packet"]
  O --> B
  A --> B
  B --> R["RUN: spawn, coordinate, barrier"] --> V{"VERIFY in parent"}
  V -- pass --> M["MERGE, CLEANUP, REPORT"]
  V -- fail --> K
  D -. "when frame goal, authority, budget, critical path" .-> FC["references/orchestration-contract.md"]
  K -. "when worker kind, model tier" .-> SG["references/spawn-gate.md"]
  C -. "when split work, topology" .-> DC["references/decompose.md"]
  B -. "when brief a worker, parse a return" .-> PK["references/packets.md"]
  R -. "when stall, failure, A2A peer" .-> CO["references/coordinate.md"]
  R -. "when peers, shared files, leases, handoffs" .-> SW["references/shared-work.md"]
  V -. "when second mind, attack, blind review, consensus" .-> CH["references/challenge.md"]
  V -. "when behavior change (TDD), improvement claim" .-> EV["references/evaluation.md"]
  M -. "when barrier, merge, cleanup, report" .-> CP["references/completion.md"]
  O -. "when offload, packet, verify gate" .-> LO["references/local-ollama.md"]
  O -. "when model ROUTE, tier, pull" .-> MS["references/model-selection.md"]
  O -. "when CLI, invoke, serving failure" .-> OL["references/ollama-cli.md"]
```
Caption: default solo; a dotted edge loads its page; every worker result passes parent VERIFY before merge.
## Rules
1. Frame substantial work before fan-out. Never broaden intent, permissions, effects, deletion scope, or budget because this skill is active.
2. Spawn only when delegation improves speed, expertise, isolation, or context quality. Otherwise work solo; batch known independent reads.
3. Give each worker one bounded objective. No nested spawn unless the host allows it and a new value/cost gate passes.
4. Check what context the worker inherits. Add only the missing goal, scope, evidence, authority, ownership, and acceptance.
5. Treat worker output as claims. Re-check load-bearing anchors in the parent; always VERIFY Ollama output. Reach the worker barrier before synthesis; keep `partial`, `blocked`, conflicts, and dissent visible.
6. Use the requested model or the host default; else pick a capable configured model. Challenge techniques use fresh context; agreement is not proof. Local Ollama is tool-less one-shot or map-reduce only.
7. Stop when acceptance is met or progress needs missing authority or information. Completed workers trigger parent verification; an empty worker list does not mean done.
Sources for these rules: `references/references.md`.
Related: `octocode-eval-benchmark` measures worker quality and this skill; `octocode-rfc-generator` before multi-agent architecture changes; `octocode-agentic-prompts` for packet contracts; `octocode-skills` for this folder.
Scripts: at GATE and after model ROUTE, run `scripts/ollama-health.sh`; at RUN, run `scripts/ollama-worker.sh` once per sealed packet or shard with `--job`, `--input`, `--schema`, `--out`, and `--keepalive`. After changing tool-using orchestration, run `scripts/eval-contract.mjs`. It validates `evals/cases.json`; `--results` grades only a fresh current-digest receipt kept outside the shipped skill.
