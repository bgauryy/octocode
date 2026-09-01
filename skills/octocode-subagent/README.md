# Octocode Orchestration

`octocode-subagent` is the stable skill identifier for accountable, platform-independent orchestration. It frames substantial work, chooses solo work, batching, or delegation, coordinates specialist workers, owns integration, and can offload low-risk sealed packets to local Ollama.

## Use it for

- Frame a substantial goal, authority, budget, critical path, and observable completion.
- Choose parent work, batching, specialist workers, remote A2A peers, or local Ollama.
- Coordinate owned workstreams and merge conflicting results.
- Challenge claims through interviews, red teams, blind review, or consensus.
- Verify integration, documentation, cleanup, and the real host or CLI path.
- Offload sealed summarize, extract, classify, translate, draft, vision, or map-reduce packets.

## Core contracts

- A spawn gate avoids delegation when parent work, a skill, or one batched call is cheaper.
- The parent owns authority, shared state, integration, verification, cleanup, and the final answer.
- Workers receive bounded packets with ownership, evidence, return shape, and stop conditions.
- Local Ollama follows `GATE → ROUTE → RUN → VERIFY → REPORT` and never receives tools or secrets.
- Behavior changes use TDD or measurable evals when ordinary checks cannot establish the outcome.

## Workflow

```text
Tool-using: FRAME → GATE → DECOMPOSE → ROUTE → PACKET → SPAWN/HANDOFF → COORDINATE → VERIFY → SYNTHESIZE → CLEANUP → REPORT
Ollama:     GATE → ROUTE → RUN → VERIFY → REPORT
```

The lobby owns the workflow; `references/` holds technique and host-neutral detail. Map `coordinate.md` actions to the host API. Measure whether fan-out helped with `octocode-eval-benchmark` and `subagent-cookbook.md`.

Run `node scripts/eval-contract.mjs` for suite integrity. Behavioral grading needs `--results` with a fresh receipt whose subject digest matches the current skill. The bundled pre-merge receipt is provenance only.

## Install

```bash
npx octocode skill install octocode-subagent
```

Add `--platform <target>` for a specific host (`pi`, `claude`, `cursor`, `codex`).
