# Octocode Subagent

Host-agnostic meta-skill for task breakdown, specialist delegation, model routing, and multi-agent coordination. Works with any spawn/Task/teammate API (Cursor, Claude, Pi, OpenAI Agents, LangGraph, A2A, …).

## When to use

- Break a large goal into parallel or staged workers
- Choose specialist vs clean worker vs stay in parent
- Route model size to task difficulty
- Coordinate wait/steer/stop across workers
- Merge conflicting worker results before answering
- Talk to remote A2A peers

## Features

- Spawn gate that prefers parent/skill/batch before multi-agent overhead
- DAG decomposition with sync-vs-async tags
- Pattern catalog: ReAct, skills, plan-execute, supervisor, handoffs, router, A2A
- Portable coordination actions (list/wait/send/steer/stop)
- Barrier synthesize with conflict-first merge
- Three-tier model routing from the host’s configured models
- Optional Octocode research + Awareness hooks

## Operating model

```text
GATE → DECOMPOSE → ROUTE → PACKET → SPAWN → COORDINATE → SYNTHESIZE → CLEANUP
```

Users get safer parallel work with clear ownership. Developers extend `references/`; lobby owns the workflow. Host-specific tool names stay out of this skill — map `coordinate.md` to the local API.

## Install

```bash
npx octocode skill --name octocode-subagent
```

Add `--platform <target>` for a specific host (`pi`, `claude`, `cursor`, `codex`).
