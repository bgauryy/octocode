# Awareness Agent Flow in Pi

Awareness has one agent-facing interface in Pi: the bundled CLI at
`$OCTOCODE_AWARENESS_CLI`, guided by the `octocode-awareness` skill. Coordination
is not duplicated as Pi tools.

## Why

One CLI/schema keeps flags, help, compact output, hooks, other coding agents, and
Pi on the same contract. The Pi bridge automates lifecycle events but writes to
the same SQLite store.

## Identity

- An explicit user `OCTOCODE_AGENT_ID` remains stable.
- Otherwise Pi derives `pi:<session-file>` for the current session.
- Sequential `/new`, `/resume`, and forked sessions refresh the derived identity.
- Hooks and CLI subprocesses inherit that same current identity.

## Start

```bash
node "$OCTOCODE_AWARENESS_CLI" attend \
  --workspace "$PWD" --query "current task" --compact
```

Inspect Ready, Claimed, Verify, FilesUnderWork, and direct signals. Recalled
memories are leads; verify them against current source/tests.

## Choose work

Claim a matching task:

```bash
node "$OCTOCODE_AWARENESS_CLI" task ready --plan-id plan_123 --limit 10 --compact
node "$OCTOCODE_AWARENESS_CLI" task claim \
  --task-id task_123 --agent-id "$OCTOCODE_AGENT_ID" --compact
```

Or open standalone Work:

```bash
node "$OCTOCODE_AWARENESS_CLI" work start \
  --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --file src/a.ts --rationale "fix parser" --test-plan "parser tests" --compact
```

Every file edit belongs to a Task/Work run with a reason and check. Advisory
presence is the default and allows informed overlap. Add `--exclusive` only for
sensitive/non-mergeable changes.

## Hooks during edits

Pi wires Awareness in process:

1. pre-edit resolves the current agent/session and target paths;
2. it heartbeats/attaches advisory file presence;
3. another run’s exclusive lease blocks the mutation;
4. post-edit records the edit and preserves the owning run;
5. finish warns about the current agent’s unresolved run.

Pi does not need `hooks install`; that command is for shell-hook hosts.

## Finish exactly owned work

For a task:

```bash
node "$OCTOCODE_AWARENESS_CLI" task submit \
  --task-id task_123 --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" \
  --message "parser tests passed" --compact
node "$OCTOCODE_AWARENESS_CLI" verify mark \
  --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" \
  --message "parser tests passed" --compact
```

For standalone Work:

```bash
node "$OCTOCODE_AWARENESS_CLI" work end \
  --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" --compact
node "$OCTOCODE_AWARENESS_CLI" verify mark \
  --run-id run_123 --agent-id "$OCTOCODE_AGENT_ID" \
  --message "reviewed diff" --compact
```

Never use a batch success operation to clear another agent’s debt. Verification
records evidence; it does not execute the check.

## Recall and record

Use targeted retrieval only when durable context can change the plan:

```bash
node "$OCTOCODE_AWARENESS_CLI" memory recall \
  --query "parser regression" --workspace "$PWD" --smart --limit 5 --compact
```

Record only reusable, verified facts:

```bash
node "$OCTOCODE_AWARENESS_CLI" memory record \
  --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --task-context "parser regression" \
  --observation "Malformed escapes must be rejected before tokenization" \
  --label GOTCHA --importance 8 --reference file:src/parser.ts --compact
```

Skip routine status, raw logs, obvious edits, secrets, and facts already captured
in source/docs.

## Signals and handoff

```bash
node "$OCTOCODE_AWARENESS_CLI" signal list \
  --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --limit 10 --compact
node "$OCTOCODE_AWARENESS_CLI" signal publish \
  --agent-id "$OCTOCODE_AGENT_ID" --to-agent other-agent --kind handoff \
  --subject "Parser task ready" --body "Run parser tests before finishing" \
  --ref-id run_123 --workspace "$PWD" --compact
```

Use a Plan Task for selectable durable work; signals are communication, not a
second task queue.

## Cleanup

`/octocode-memory-digest` and `/octocode-memory-forget` are user-owned maintenance
commands. They preview by default and mutate only after confirmation. Repo
projection is optional and should run last, only when file readers need refresh.
