# Octocode Awareness skill

This skill ships with `@octocodeai/octocode-awareness`. `SKILL.md` is the operating entry point; it routes to a small reference only when shared state changes the next action.

The routine flow is:

```text
ORIENT -> ACT -> COORDINATE WHEN DECISION-CHANGING -> VERIFY -> RECOVER IF NEEDED
```

Use the host-bound Awareness tool when available. Otherwise call the public CLI with the same database, workspace, and stable actor identity supplied by the host:

```bash
npx @octocodeai/octocode-awareness context orient \
    --workspace "$PWD" \
    --agent-id "awareness:session-1" \
    --compact
```

Discover the live routine contract only when needed:

```bash
npx @octocodeai/octocode-awareness schema commands --compact
npx @octocodeai/octocode-awareness schema command context orient --compact
```

The CLI exposes exactly nineteen operations across Context, Work, Message, Memory, and History. Unknown operation names fail.

## Reference map

| Need | Reference |
|---|---|
| Choose an operation | [Flow matrix](references/flow-matrix.md) |
| Track shared ownership and dependencies | [Shared Work](references/plan-task-workflow.md) |
| Coordinate with a peer | [Message protocol](references/coordination-protocol.md) |
| Protect a non-mergeable path | [Exclusive path protection](references/lock-protocol.md) |
| Recall or record reusable evidence | [Memory evidence](references/memory-recall.md) |
| Inspect or restore recoverable bytes | [Local History](references/local-history.md) |
| Understand storage and host boundaries | [Awareness architecture](references/architecture.md) |
| Understand workspace policy | [Awareness configuration](references/configuration.md) |
| Understand host lifecycle behavior | [Lifecycle hooks](references/hooks.md) |
| Route artifacts to one owner | [Output routing](references/output-routing.md) |
| Research code or repository evidence | [Octocode research operations](references/octocode.md) |

## Generated runtime assets

The package build emits the CLI and hook runtime into `out/` and mirrors the skill into generated destinations. Edit the package source, `SKILL.md`, or the reference source instead of generated bundles.

| Asset | Purpose |
|---|---|
| `scripts/awareness.mjs` | Bundled canonical CLI |
| `scripts/hook-runner.mjs` | Shell-host lifecycle adapter |
| `scripts/extract-hook-files.mjs` | Bounded host payload path extraction |
| `scripts/hooks/*.sh` | Thin lifecycle wrappers |
| `agents/openai.yaml` | Skill interface metadata |
| `evals/trigger-cases.json` | Activation regression cases |

Pi uses native lifecycle events and must not also run shell hooks. Other supported hosts use shell hooks only when their workspace policy selects `shell` ownership and installation has been authorized outside the routine Awareness surface.

## Verification

From the monorepo, run:

```bash
yarn workspace @octocodeai/octocode-awareness build
yarn workspace @octocodeai/octocode-awareness test:quiet
```

For a host adapter change, also verify one real lifecycle event in that host. Configuration text alone does not prove activation.
