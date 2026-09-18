# AGENTS.md — @octocodeai/octocode-awareness

This package dogfoods shared work, verification, memory, hooks, and generated repo
context. `AGENTS.md` routes maintainers; the skill owns operating policy; the package
API owns live state/contracts shared with the CLI; docs own architecture and feature depth.

## Enter

Activate `octocode-awareness`, choose one stable identity, then inspect the shared
ledger. Every agent-facing CLI example uses the published package runner; host
integrations call the package API directly. Build before testing changed
package code, then verify the public runner separately.

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-awareness:$(node -e 'process.stdout.write(require("node:crypto").randomUUID())')}"
npx @octocodeai/octocode-awareness context orient --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --compact
```

Follow typed `next` results; use `schema command <concept> <operation> --compact`
for unclear fields. SQLite is canonical. Never hand-edit generated `.octocode/`
state.

Orient once per workspace/session, or reuse the native host briefing. Communicate
when a peer needs to know or act. Discover tracking, locks, memory, or recovery only
when useful. For tracked work, reuse task/run IDs, record observed checks, and audit
after final writes. Use locks only for unsafe concurrent work; never bypass a conflict.

## Package Constraints

- Edit runtime/CLI and Zod contracts in `src/**` and `bin/**`.
- Edit the canonical skill in package-local `skills/octocode-awareness/**`.
- Edit package guidance in `README.md` and `docs/**`.
- Never hand-edit `out/**`, `.agents/skills/**`, or generated helpers/schemas under
  `skills/octocode-awareness/scripts/**`.
- `out/**` is the ignored publishable build tree; do not restore `dist/**` or a
  repository-root Awareness skill source tree.
- When work needs tracking, declare edited paths on the owning run. Guard/full
  hooks can automate this; default coordination hooks only deliver messages.
- Before planning, recall memory only when prior learning could change the approach;
  filter by workspace/artifact/file/label and treat ranked hits as leads to verify.
- Harness changes require user authorization, `OCTOCODE_ALLOW_HARNESS_APPLY=1`,
  and a safe non-main branch.
- Keep one normalized workspace and agent ID. Store no secrets in Awareness rows or
  projections.

After any source or skill edit, rebuild before using the CLI, hooks, smoke scripts,
or mirrors:

```bash
yarn workspace @octocodeai/octocode-awareness build
```

## Verification

Use `docs/VERIFY.md` for the complete quick/installed/host/monorepo/release runbook.
Use TDD and the smallest focused check first. Broaden shared changes before marking
the run verified:

```bash
yarn workspace @octocodeai/octocode-awareness typecheck
yarn workspace @octocodeai/octocode-awareness test:quiet
yarn workspace @octocodeai/octocode-awareness pack:check
yarn workspace @octocodeai/octocode-awareness verify
```

Skill changes also require `yarn workspace @octocodeai/octocode-awareness build`
and focused tests. Preserve failed-check evidence. Record only reusable learning.
Hooks: `docs/HOOKS.md`; lifecycle: `docs/HOW_IT_WORKS.md`; concept owners:
`docs/README.md`.

## Docs

- [Native API and prompt exports](docs/API.md)
- [Configuration and integration ownership](docs/CONFIGURATION.md)
- [Shared host primitives](src/host-api.ts)
- [Pi integration](../octocode-pi-extension/docs/AWARENESS_AGENT_FLOW.md)
