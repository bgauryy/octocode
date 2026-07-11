# AGENTS.md — @octocodeai/octocode-awareness

This package dogfoods shared work, verification, memory, hooks, and generated repo
context. `AGENTS.md` routes maintainers; the Awareness skill owns operating policy;
the CLI owns live state and contracts; hooks automate lifecycle edges; package docs
own architecture and feature depth.

## Enter

Activate `octocode-awareness`, export one stable identity, then ask live state for
the next action:

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-codex-awareness}"
AWARENESS="node packages/octocode-awareness/out/octocode-awareness.js"
$AWARENESS attend --workspace "$PWD" --query "<current task>" \
  --agent-id "$OCTOCODE_AGENT_ID" --compact
```

Always follow `attend.next`. Load focused help, one skill reference, or one package
doc only when that action needs more detail. Do not preload inventories.

Use `.octocode/` plan docs/indexes as a menu, not state. Confirm live work with
`attend`/`query`, learning with `memory recall --smart`, and guidance with `docs show`.
Never edit projections; only authored `.octocode/plan/**` docs are source material.

If the skill or hooks are unavailable, keep the manual fallback: attend; declare
edited paths with `work start`; run the declared check while presence is active;
then `work end`, `verify mark`, and `verify audit`. Ordinary overlap is advisory;
use exclusivity only for sensitive, non-mergeable work and never bypass a conflict.

## Package Constraints

- Edit runtime/CLI and Zod contracts in `src/**` and `bin/**`.
- Edit the canonical skill only in repo-root `skills/octocode-awareness/**`.
- Edit package guidance in `README.md` and `docs/**`.
- Never hand-edit `out/**`, `.agents/skills/**`, or build-generated Awareness
  helpers/schemas under repo-root `skills/octocode-awareness/scripts/**`.
- `out/**` is the ignored, publishable build tree: separate CLI, import-only
  library/schema API, declarations, per-contract JSON schemas, and bundled skills.
  Do not restore `dist/**` or a package-local `skills/**` source tree.
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

Skill changes also require:

```bash
node skills/octocode-skills/scripts/skill-review.mjs \
  skills/octocode-awareness
```

Preserve failed-check evidence. Record only reusable learning. The executable user
flow lives in `docs/SKILLS.md`; host automation in `docs/HOOKS.md`; architecture and
the complete lifecycle in `docs/HOW_IT_WORKS.md`; all concept owners in `docs/README.md`.
