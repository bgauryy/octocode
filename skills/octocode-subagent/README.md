# Octocode Subagent

Host-agnostic meta-skill for task breakdown, specialist delegation, model routing, multi-agent coordination, **and** local Ollama token offload. Works with any spawn/Task/teammate API (Cursor, Claude, Pi, OpenAI Agents, LangGraph, A2A, …) plus sealed-packet Ollama one-shots.

## When to use

- Break a large goal into parallel or staged workers
- Choose specialist vs clean worker vs stay in parent vs local Ollama
- Route model size to task difficulty (host tiers or `ollama list`)
- Coordinate wait/steer/stop across workers
- Merge conflicting worker results before answering
- Talk to remote A2A peers
- Challenge claims with rubber-duck, interview, mimic-flow, red-team, blind review, consensus
- Save tokens: summarize/extract/classify/translate/draft/check/vision/map-reduce on saved text/images

## Features

- Spawn gate that prefers parent/skill/batch before multi-agent overhead
- Local Ollama path: `GATE → ROUTE → RUN → VERIFY → REPORT` (`references/local-ollama.md`)
- DAG decomposition with sync-vs-async tags
- Pattern catalog: ReAct, skills, plan-execute, supervisor, handoffs, router, A2A, Ollama offload
- Challenge techniques: rubber duck, interview, mimic-flow, red-team/premortem, blind review, consensus
- Portable coordination actions (list/wait/send/steer/stop)
- Barrier synthesize with conflict-first merge + output decision cards
- Three-tier model routing from the host’s configured models
- Optional Octocode research tooling for worker evidence

## Operating model

```text
Tool-using: GATE → DECOMPOSE → ROUTE → PACKET → SPAWN → COORDINATE → SYNTHESIZE → CLEANUP
Ollama:     GATE → ROUTE → RUN → VERIFY → REPORT
```

Users get safer parallel work with clear ownership. Developers extend `references/`; lobby owns the workflow. Host-specific tool names stay out of this skill — map `coordinate.md` to the local API. Measuring whether fan-out helped → `octocode-eval-benchmark` (`subagent-cookbook.md`).

## Install

```bash
npx octocode skill --name octocode-subagent
```

Add `--platform <target>` for a specific host (`pi`, `claude`, `cursor`, `codex`).
