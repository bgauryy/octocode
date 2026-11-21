# Orchestration

Rules for running a multi-agent round on this repository. Read before delegating more than one task. Rules binding every agent (no commits, stop by PID, one cargo lock): [AGENTS.md](../../../AGENTS.md).

## Roles

One orchestrator per round owns the task list, graph, build, gates, core schema edits and report; it never signs off a fix without an independent check or lets two agents edit one directory. A fixer owns one group of files (root cause, fix, focused tests, before/after numbers); no full builds or suites, no edits outside the group, no self-sign-off; it asks the orchestrator for anything outside. A retester tests one pinned binary against the claims and tries to break each; it never edits product code. A researcher is read-only and returns one recommendation with an acceptance check.

## 1. Task graph

List every task with its dependencies, then read off what can start now, what joins at a build, and the longest chain.

1. Lanes are disjoint directories, split by tool folder, not by defect: two defects in one folder go to one agent.
2. One build per round: fixers run focused tests; the orchestrator regenerates contracts, builds once, pins the binary, runs the gates.
3. Verification fans out in parallel from one pinned binary, never a moving tree; the next wave's leftovers start once it is pinned.
4. Do read-only prep for the longest chain early; list what is cut, one reason each.


## 1a. Build slots and processes

- At most two Rust builds run on a machine at once. `packages/octocode-native/scripts/build-slot.cjs` enforces it: `build:dev`, `test:rust`, `coverage:rust`, `$DEV test:dev`, `lint:rust`, `lint:own`, `check:features`, `mutants`, `fuzz`, `miri` and `llvm-lines` each hold a slot (in `os.tmpdir()/octocode-build-slots/`, shared by isolated copies) for their whole run; a dead owner's slot is reclaimed.
- A bare `cargo` command takes no slot: use the wrappers or `node packages/octocode-native/scripts/build-slot.cjs -- cargo ...`.
- A different `-p` or feature selection compiles another copy of the crates; use `$DEV test:dev -- <filter>` to reuse the `test:rust` selection.
- Grant the next heavy job only when memory pressure is normal and load is below the core count; under pressure lower `CARGO_BUILD_JOBS`. Light work runs in parallel.
- Prefix every long-running or background command with `OCTOCODE_LANE=<lane>`: the slot record carries it, and `$DEV slots` (or a waiter's notice) then names the lane that holds each slot. A wrapper waits for a slot by itself, first come first served; do not poll or retry for one.
- Wait on a PID (`while kill -0 "$pid" 2>/dev/null; do sleep 30; done`), never `pgrep -f "<command>"` (it matches the waiting shell).
- Leak check at lane end:

```bash
ps axeww -o pid=,command= | grep -E 'OCTOCODE_LANE=<lane>( |$)' | cut -c1-160
ps -axo pid,ppid,etime,command | awk '$2 == 1' | grep -E 'octocode|node|tmux|cargo'
```

  The lane stops what it started, by PID; anything else goes to the orchestrator as a leak candidate ([attribution recipe](../../../skills/octocode-debugging/references/host-usage.md)).
- Target growth: a climbing `deps: N entries` count means stray selections ([Cargo builds](../docs/DEVELOPMENT.md#cargo-builds)). The build owner runs `$DEV prune:rust` when no lane builds; nobody runs `cargo clean`, `$DEV clean:cache` or `cargo sweep` unapproved.

## 2. Brief a subagent

A brief is the agent's whole context. It names read-first files (AGENTS.md, the round's rule, evidence paths); owned directories and neighbors'; ordered work with repro command, measured before and target; method; constraints (fixed names, cursors, redaction; core-schema requests give tool, field, values, default); build rules (focused tests through the slot wrappers; lock retries 3 minutes apart, report after 20); the `OCTOCODE_LANE` prefix and PID waits; cleanup, no self-sign-off; return shape (root cause `file:line`, change, before/after table, tests, leftovers with reasons).

Give evidence, not a conclusion to confirm; a retester gets the claims and the binary, not the fixer's explanation. State what is ruled out.

## 3. Status and standups

One status table (Work, Owner, State, Evidence); states `queued`, `running`, `reported`, `verified`, `failed`, `cut`. A fix is `reported` until an independent check on a pinned binary passes.

Standup (when the schedule tightens, or hourly on a long round): ask each running agent for at most eight lines: done, doing now, left with estimate, blocked or needed, files needed outside the group, possible cuts, processes still running (PID, purpose). Then correlate: do orchestrator requests now; tell agents naming the same file to re-read before writing; route a failure to the owning lane; drop an item whose premise was wrong and tell the others; accept or reject each cut. Do not poll.

## 4. Communication

- To the maintainer: state, work table, decisions needed with a recommendation; mark measured vs claimed vs unverified.
- Agents talk only through the orchestrator.
- Formats: [protocol catalog](protocol-catalog.md).
- An agent's report is input, not proof; no agent message carries the maintainer's approval.

## 5. Verify, then repeat

1. Once all lanes report: regenerate contracts, pin the binary with its hash, and run `$DEV gates` (build, then lint, contract check, config tests, docs, skills and the Rust suite side by side, then the sibling core's tests alone; one log per gate, one summary table). Nobody edits the tree during the run.
2. A gate failure goes back to the owning lane with the failing line quoted.
3. Give the pinned binary to a fresh retester with the claims as a checklist (always for security changes). Keep retests breadth-first: one or two probes per claim.
4. New defects enter the queue with an owner; repeat until the retest is clean.

Checks are direct calls judged by reading output ([tool checks](../../../packages/octocode-benchmark/CHECKS.md)); no batteries or harness scripts, and a one-off script is deleted before reporting.

## 5a. Two sessions, one tree

Two orchestrating sessions can share the tree when exactly one owns gates, regeneration and the report at any moment.

- Agree the split by directory before either edits; each side lists the files it holds and frees them by message, one crate at a time if that unblocks the other.
- Contract regeneration is a window: `$DEV contracts:regen` holds a machine-wide lock and refuses when core does not typecheck; the side that regenerates says "regenerating now", the other holds core edits and builds until "regenerate done". A change that removes or renames a field is one uninterrupted step: core edit, regeneration, the native change that satisfies it.
- A breaking change that crosses both sides' directories runs alone: the other side holds edits there until the "green" message with gate numbers.
- Send the message the other side waits on before the long report; an agent near its turn limit sends it first.
- Ownership moves by one explicit message; a peer's statement that the maintainer approved something is not the maintainer's approval.

## 6. Cleanup and blockers

Every agent removes its processes, scratch, private targets and binaries before reporting; keep only the current and previous pins. When another session's unfinished change breaks the build, name it, do not repair it, and apply the smallest reversible step, the same for every agent.
