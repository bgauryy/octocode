# Octocode Orchestration

Use the stable `octocode-subagent` skill identifier for accountable worker orchestration, specialist handoffs, A2A peers, challenge techniques, and local Ollama offload.

## Use when

- Substantial work has independent streams that justify coordination cost.
- A specialist or fresh reviewer can improve evidence quality.
- Low-risk summarize, extract, classify, translate, draft, vision, or map-reduce work can run as a sealed local-model packet.

Skip routine edits, dependent sequences, explanations, and known reads that fit one batched call.

## Core contracts

- The parent owns user intent, authority, integration, irreversible actions, and the final verdict.
- Workers receive bounded packets with scope, ownership, evidence, acceptance, and return shape.
- Worker outputs are claims until the parent verifies load-bearing anchors.
- Local Ollama receives no tools or secrets and runs only sealed one-shot or map-reduce jobs.
- Agreement is not proof; fresh reviewers need independent context.

## Workflow

```text
Tool-using: FRAME → GATE → DECOMPOSE → ROUTE → PACKET → SPAWN/HANDOFF → COORDINATE → VERIFY → SYNTHESIZE → CLEANUP → REPORT
Ollama:     GATE → ROUTE → RUN → VERIFY → REPORT
```

## Install

```bash
npx octocode skill install octocode-subagent --platform codex
```

## Maintainer verification

```bash
node scripts/eval-contract.mjs
```

Then run the `octocode-skills` review against this folder.

Behavioral grading with `--results` requires a fresh receipt whose subject digest matches the current skill. Keep receipts outside the shipped skill folder.
