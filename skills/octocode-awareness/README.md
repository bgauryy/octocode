# Octocode Awareness

`octocode-awareness` gives agents situational awareness in a real local repo. It stores local memory, file claims, handoffs, peer messages, and verification records in SQLite so separate runs can coordinate instead of starting cold or colliding silently.

The core loop is simple: read before acting, claim files before editing, verify before saying done.

## How it works

The skill starts by recalling relevant memories, refinements, active locks, and unread peer messages for the current workspace. Before edits it records a pre-flight intent for the target files, then after the work it records verification, releases locks, and saves reusable lessons or handoffs when the next agent would benefit.

## Good asks

- "Use awareness before changing this repo."
- "Check whether another agent is working on these files."
- "Remember the lesson from this failed test for next time."
- "Leave a handoff for the next agent on this branch."
- "Show me the awareness data."

## What you get

- Reusable memories for lessons, gotchas, workflows, and decisions.
- Repo and branch-specific handoffs for unfinished work.
- File locks that say who claimed a file, why, and when the claim expires.
- Agent-to-agent messages for blockers, questions, decisions, and handoffs.
- Verification records tying an edit intent to the test or review plan that actually ran.
- A local HTML viewer when a human wants to inspect the stored state.

## Where it fits

Use Awareness alongside editing or investigation skills when the repo is dirty, the task is long-running, multiple agents may touch the same files, or the user wants durable lessons. It is not a code-search skill and it does not replace tests; it makes coordination and verification visible.

## Hook-aware behavior

In hosts that support lifecycle hooks, Awareness can automatically claim files before edits, release locks afterward, surface unread messages, capture session handoffs, and flag unverified "done" claims. Without hooks, agents can run the same commands manually through `scripts/awareness.py`; the data model stays the same.

## User value

The user gets calmer multi-agent work: fewer hidden collisions, less repeated rediscovery, clearer handoffs, and success claims backed by recorded verification instead of good intentions.
