# Session Observability

Use live DB timestamps, not projection mtime alone. Compare run-file heartbeat/expiry,
locks, task claims, row updates, and file mtimes.

| Question | Read |
|---|---|
| Active peers | `work list|show` |
| Plans/tasks/runs/locks | `workspace status`, workboard |
| Missing references | `query files` |
| Verification debt | `verify audit` |
| Human cross-view | `query all --format html` |
| Cleanup impact | `maintenance digest --dry-run` |

`session capture` writes a `quality=handoff` refinement from unresolved work and dirty
files. Content fingerprinting prevents repeated SessionEnd/PreCompact events from
duplicating the same handoff.

Claude uses SessionEnd; Codex uses PreCompact; Cursor uses sessionEnd/preCompact; Pi
uses shutdown/pre-compact. Hooks fail open, so capture manually before risky handoff.

A host session is not a work-unit boundary. Task claim or explicit `work start`
defines run reuse. Close a handoff by applying/verifying its action and marking the
same refinement done.

Collision decisions: `files-awareness.md`; output choice: `output-routing.md`.
