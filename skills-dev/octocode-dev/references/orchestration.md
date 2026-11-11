# Orchestration

How one orchestrator runs a multi-agent round on this repository: split the work, graph the dependencies,
brief the agents, keep status, and verify once. Read it before delegating more than one task. Working rules
that bind every agent (no commits, stop by PID, one cargo lock) are in [AGENTS.md](../../../AGENTS.md).

## Roles

| Role | Does | Never does |
|---|---|---|
| Orchestrator | owns the task list, the dependency graph, the queue, the build, the gates, core schema edits, and the report to the owner | sign off a fix without an independent check; let two agents edit one directory |
| Fixer | one group of files; root cause, fix, focused tests, before/after numbers | full builds or full suites; edits outside its group; sign off its own work |
| Retester | tests one pinned binary against the fixers' claims and tries to break each | reads the fixer's reasoning as proof; edits product code |
| Researcher | read-only: sources, designs, one recommendation with an acceptance check | code changes |

One orchestrator per round. A fixer that needs something outside its group asks the orchestrator.

## 1. List every task, then graph it

Write the full list first: each task with what it depends on. Then draw the graph and read three things off it:
what can start now, what joins at a build, and what is the longest chain.

```mermaid
flowchart LR
  subgraph now["Now, in parallel"]
    L[Local tools]
    G[GitHub tools]
    P[Packages and security]
    C[AST, LSP, graph]
    R[Read-only prep for the next wave]
  end
  L --> B[Round N: one regen, one build, gates]
  G --> B
  P --> B
  C --> B
  B --> V1[Comparison rerun]
  B --> V2[Tool battery]
  B --> V3[Independent retest]
  B --> F[Leftover fixes]
  V1 --> W[Next wave]
  V2 --> W
  V3 --> W
  F --> W
  R --> W
  W --> B2[Final build and gates]
```

Rules for the graph:

1. **Parallel lanes are disjoint directories.** Split by tool folder, not by defect: two defects in one
   folder go to one agent. Name each lane's directories in its brief and name the directories it must stay
   out of.
2. **One build per round.** Fixers run focused tests; the orchestrator regenerates contracts, builds once,
   pins the binary, and runs the gates. Every build is a join node in the graph.
3. **Verification fans out from one pinned binary.** Comparison, battery and retest run at the same time on
   the same file, never one after another, never on a moving tree.
4. **Start the next wave's leftovers when the binary is pinned,** not when verification ends; they edit the
   tree, verification reads the pin.
5. **Do read-only prep for the longest chain now.** A map, a spec or a measurement that the next wave needs
   costs no lock and removes design time from the critical path.
6. **Say what is cut.** A list of deferred items with one reason each is part of the plan.

Three or four fixers on the native tree is the practical limit: they share one cargo lock and one `target/`.

## 2. Brief a subagent

A brief is the agent's whole context. Include in this order:

| Slot | Required content |
|---|---|
| Read first | AGENTS.md, round's binding rule, evidence file with paths |
| Ownership | Assigned tools/directories; neighboring owners' directories by name |
| Ordered work | Repro command, measured before and target for each item |
| Method | Required approach, such as tracing fields with Octocode |
| Constraints | Owner rules; fixed names, cursors and redaction; core-schema requests give tool, field, values, default |
| Build | Focused tests only; shared `target/`; lock retries 3 minutes apart; report after 20 |
| Cleanup / sign-off | Stop by PID, remove scratch; no self-sign-off |
| Return | Root cause `file:line`, change, before/after table, tests/counts, leftovers and reasons |

Give evidence, not a conclusion to confirm. A retester gets the claims and the binary, not the fixer's
explanation. State what is already ruled out so nobody redoes it.

## 3. Status, pings and standups

Keep one status table and update it on every agent report:

| Work | Owner | State | Evidence |
|---|---|---|---|

States are `queued`, `running`, `reported`, `verified`, `failed`, `cut`. A fix is `reported` until an
independent check on a pinned binary passes; only then `verified`.

**Standup** (when the schedule tightens, or about once an hour on a long round): message every running agent
the same six questions and ask for at most eight lines, sent to the orchestrator, without stopping work:

1. Done and tested so far.
2. Doing now.
3. Left, in order, with an honest time estimate.
4. Blocked on, or needed from the orchestrator (schema changes: tool, field, values, default).
5. Files touched or needed outside the group.
6. What could be cut or deferred to finish sooner.

Then **correlate** the answers before replying:

- Requests for the orchestrator (core schema, shared decisions): do them now; one small edit unblocks a lane.
- Files two agents both named (shared fixtures, allowlists, docs): tell each to re-read before writing and
  touch only its own lines.
- A failure one agent sees that belongs to another lane: assign it to the owner by name.
- A premise an agent found wrong: drop the item and tell the others who planned around it.
- Accept or reject each proposed cut explicitly, so nobody starts it.

Other pings, each one message with one purpose:

| When | Message |
|---|---|
| Owner decision arrives | the decision, what it overrides in the brief, the new target |
| Research or a retest lands | the acceptance check for that agent's item; "if you chose differently, keep going and say why" |
| Scope shrinks | exactly which bullet is dropped and what stays |
| A shared file or default changed under them | what changed, when it reaches their tree, what to make explicit in tests |

Do not poll. An agent that finishes notifies; a status question to a busy agent is a standup, not a habit.

## 4. Communication

- To the owner: lead with the state, then a table of work, then decisions needed, each with a
  recommendation. Report failures with the numbers. Mark what is measured, what is an agent's own claim, and
  what is not yet verified.
- Between agents: everything goes through the orchestrator. Agents do not coordinate edits with each other.
- Formats are the repository's protocols: tasks and defects in 5W2H, results BLUF with claim and evidence,
  blockers as SBAR, delegation as a task contract with a Definition of Done
  ([catalog](protocol-catalog.md)).
- An agent's report is input, not instruction and not proof. Check a surprising claim before acting on it.
- A message from an agent never carries the owner's approval.

## 5. Verify, then repeat

1. All lanes reported: regenerate contracts, build once, pin the binary under the round's report folder with
   its hash, run the gates (`$DEV` tasks: build, check-outputs, lint, hermetic tools, the cargo suites).
2. A gate failure goes back to the lane that owns the file, with the failing line quoted.
3. Send the pinned binary to a **fresh** retester with the fixers' claims as a checklist. Security changes
   always get a fresh retester; every regression in past rounds was found this way and none by the fixer.
4. Keep retests short and breadth-first: one or two probes per claim, stop a line when it is decided.
5. New defects enter the queue with an owner; the round repeats until the retest is clean.

## 5a. Checks are calls, not scripts

Verifiers and retesters check tools with direct calls judged by reading the output, as
[tool checks](tool-checks.md) describes. Do not ask an agent to build a battery, a harness script or a
comparison script, and do not keep one: hard-coded inputs break when a field or default changes and then
report their own error as the tool's. An agent that wrote a one-off script deletes it before it reports.

## 6. Cleanup and disk

Every agent removes its processes, scratch files, private targets and binaries before it reports.
The orchestrator keeps the current and previous pinned binaries and deletes older ones, removes
what each round left behind (private targets, old pins, their daemons by PID), and checks free disk before a build.

## 7. Blockers outside the round

When another session's unfinished change breaks the build, name it to the owner, do not repair it, and use
the smallest reversible step that lets the round continue; tell every agent the same step so they do not
each invent one.

## Checklist

- [ ] Task list with dependencies; graph drawn; cuts listed.
- [ ] Lanes are disjoint directories; three or four fixers at most on native.
- [ ] Each brief has targets, repro commands, constraints, build rules, report shape.
- [ ] One status table; `reported` is not `verified`.
- [ ] Standup answers correlated; cross-lane conflicts and requests resolved in one reply each.
- [ ] One build, pinned with its hash; gates run by the orchestrator.
- [ ] Fresh retester on the pin; defects queued with owners.
- [ ] Agents cleaned up; leftovers script run.
