# Octocode Subagent

Meta-skill for task breakdown, specialist spawning, model routing, and multi-agent coordination on Octocode/Pi — with portable patterns from LangChain/LangGraph, OpenAI handoffs, A2A, and MAST failure research.

## When to use

- Break a large goal into parallel or staged workers
- Choose `spawnSubagent` vs `spawnAgent` vs stay in parent
- Route model size to task difficulty
- Coordinate with `AgentMessage` or remote A2A peers
- Merge conflicting worker results before answering
- Recover from failed or conflicting workers

## Features

- Spawn gate that prefers parent/skill/batch before multi-agent overhead
- DAG decomposition with sync-vs-async tags and independence tests
- Pattern catalog: ReAct, skills, plan-execute, supervisor, handoffs, router, A2A
- Correct Pi API: no `skills` on `spawnAgent`; `wait` includes idle; chrome gate
- Browser routing: `chromeDebug` / `browserAgent` / `browser-agent`
- Barrier synthesize with conflict-first merge and verifier independence
- Three-tier model routing from the live configured model table
- Recovery escalation + MAST failure watches
- Awareness worker identity formula for shared-repo writes

## Operating model

```text
GATE → DECOMPOSE → ROUTE → PACKET → SPAWN → COORDINATE → SYNTHESIZE → CLEANUP
```

Users get safer parallel work with clear ownership. Developers extend `references/` only; lobby owns the workflow. Pi bundles this skill from `skills/octocode-subagent` on extension build. SoT for tools is TypeScript sources, not `TOOLS.md`.

## Install

```bash
npx octocode skill --name octocode-subagent
```

Add `--platform <target>` for a specific host (`pi`, `claude`, `cursor`, `codex`).
