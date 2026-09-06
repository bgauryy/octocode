# AGENTS.md — @octocodeai/octocode-awareness

This package dogfoods shared work, verification, memory, hooks, and generated repo
context. `AGENTS.md` routes maintainers; the skill owns operating policy; the CLI
owns live state/contracts; package docs own architecture and feature depth.

## Enter

Activate `octocode-awareness`, choose one stable identity, then inspect the shared
ledger. Every agent-facing CLI example uses the published package runner; host
integrations may call the same package API in-process. Build before testing changed
package code, then verify the public runner separately.

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-codex-awareness}"
npx @octocodeai/octocode-awareness status --workspace "$PWD" --compact
npx @octocodeai/octocode-awareness schema commands --all --compact
npx @octocodeai/octocode-awareness schema entities --compact
```

Follow typed `next` results; use `schema command <noun> [action]` for unclear flags.
SQLite is canonical. Never hand-edit generated `.octocode/` state; only
workspace-root `.octocode/REFLECT.md` is authored reflection.

Shared fallback: `attend`; task `claim`; work `start`; run the check while present;
task `submit`; work `end`; `verify mark`. Overlap is advisory; use a lock only for
unsafe, non-mergeable, or sensitive work and never bypass a conflict.

The root commands and host adapters use one canonical ledger and lifecycle.
Use `schema command <noun> [action]` for the supported route and flags.

## Package Constraints

- Edit runtime/CLI and Zod contracts in `src/**` and `bin/**`.
- Edit the canonical skill in package-local `skills/octocode-awareness/**`.
- Edit package guidance in `README.md` and `docs/**`.
- Never hand-edit `out/**`, `.agents/skills/**`, or generated helpers/schemas under
  `skills/octocode-awareness/scripts/**`.
- `out/**` is the ignored publishable build tree; do not restore `dist/**` or a
  repository-root Awareness skill source tree.
- Declare every edited file. Structured-write hooks automate presence when healthy;
  explicit CLI presence remains the fallback.
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
yarn workspace @octocodeai/octocode-awareness test:smoke
yarn workspace @octocodeai/octocode-awareness pack:check
yarn workspace @octocodeai/octocode-awareness verify
```

Skill changes also require `yarn workspace @octocodeai/octocode-awareness build`
and focused tests. Preserve failed-check evidence. Record only reusable learning.
Executable flow: `docs/SKILLS.md`; hooks: `docs/HOOKS.md`; lifecycle:
`docs/HOW_IT_WORKS.md`; concept owners: `docs/README.md`.
