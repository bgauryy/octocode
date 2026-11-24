# Orchestration

Repo-specific facts for a multi-agent round. Ownership, gates, regeneration windows, retests and messages: `<repo>/AGENTS.md` ("Many agents, one tree").

- **Lanes** are disjoint directories split by tool folder (`crates/tool-<family>/src/<tool>/`), not by defect: two defects in one folder go to one agent. Two orchestrating sessions agree the split by directory before either edits, and free files by message, one crate at a time if that unblocks the other. A breaking change that crosses both sides runs alone until the "green" message with gate numbers.
- **One build per round.** Fixers run focused tests only (`$DEV test:dev --crate <name> -- <filter>`, prefixed with `OCTOCODE_LANE=<lane>`); the owner regenerates, builds once, pins the binary with `$DEV pin` (copies it, prints its path and sha256, keeps the current and previous pin) and runs `$DEV gates`. Every retest uses that pin, never the moving tree; security changes always get one. A gate failure goes back to its lane with the failing line quoted.
- **Brief** each agent with its owned directories and its neighbors', the reproducing call and measured baseline, and the return: root cause `file:line`, change, before → after numbers, tests run, leftovers. Give evidence, not a conclusion to confirm.
- **Load**: under memory pressure lower `CARGO_BUILD_JOBS` before granting the next heavy job. A climbing `deps: N entries` line from the build wrappers means stray selections; the build owner runs `$DEV prune:rust` when no lane builds.
- **Processes**: wait on a PID (`while kill -0 "$pid" 2>/dev/null; do sleep 30; done`), never `pgrep -f "<command>"`, which matches the waiting shell. Leak check at lane end:

```bash
ps axeww -o pid=,command= | grep -E 'OCTOCODE_LANE=<lane>( |$)' | cut -c1-160
ps -axo pid,ppid,etime,command | awk '$2 == 1' | grep -E 'octocode|node|tmux|cargo'
```

  The lane stops what it started, by PID; anything else goes to the owner as a leak candidate.
- **Another session's break**: name it and do not repair it.
