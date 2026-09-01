# Octocode Orchestration

`octocode-subagent` is the stable skill identifier for accountable, host-agnostic orchestration. It frames substantial work, chooses solo/batch/delegation, coordinates specialist workers, owns integrated verification and cleanup, and can offload low-risk sealed packets to local Ollama.

## When to use

- Frame a substantial goal, authority ceiling, budget, critical path, and observable completion
- Break a large goal into independent or staged workers
- Choose specialist vs clean worker vs stay in parent vs local Ollama
- Route model size to task difficulty (host tiers or `ollama list`)
- Coordinate wait/steer/stop across workers
- Merge conflicting worker results before answering
- Route behavior changes through TDD and measurable evals when ordinary checks are insufficient
- Coordinate shared repository state through Awareness when peers or locks can change the next action
- Verify the integrated result, documentation, cleanup, and real host/CLI path before reporting done
- Talk to remote A2A peers
- Challenge claims with rubber-duck, interview, mimic-flow, red-team, blind review, consensus
- Save tokens: summarize/extract/classify/translate/draft/check/vision/map-reduce on saved text/images

## Features

- Spawn gate that prefers parent/skill/batch before multi-agent overhead
- Orchestration contract for goal, authority, budget, ownership, and critical path
- Local Ollama path: `GATE → ROUTE → RUN → VERIFY → REPORT` (`references/local-ollama.md`)
- DAG decomposition with sync-vs-async tags
- Pattern catalog: ReAct, skills, plan-execute, supervisor, handoffs, router, A2A, Ollama offload
- Challenge techniques: rubber duck, interview, mimic-flow, red-team/premortem, blind review, consensus
- Portable coordination actions (list/wait/send/steer/stop)
- Barrier synthesize with conflict-first merge + output decision cards
- Parent-owned completion gate for integrated evidence, docs, authorized cleanup, and final reporting
- Three-tier model routing from the host’s configured models
- Optional Octocode research tooling for worker evidence

## Operating model

```text
Tool-using: FRAME → GATE → DECOMPOSE → ROUTE → PACKET → SPAWN/HANDOFF → COORDINATE → VERIFY → SYNTHESIZE → CLEANUP → REPORT
Ollama:     GATE → ROUTE → RUN → VERIFY → REPORT
```

Users get safer parallel work with clear ownership. Developers extend `references/`; lobby owns the workflow. Host-specific tool names stay out of this skill — map `coordinate.md` to the local API. Measuring whether fan-out helped → `octocode-eval-benchmark` (`subagent-cookbook.md`).

The merged orchestration suite is in `evals/`. Run `node scripts/eval-contract.mjs` for suite integrity; grading behavior requires `--results` with a fresh receipt matching the current subject digest. The included pre-merge 23/23 receipt is provenance, not a post-merge claim.

## Install

```bash
npx octocode skill --name octocode-subagent
```

Add `--platform <target>` for a specific host (`pi`, `claude`, `cursor`, `codex`).
