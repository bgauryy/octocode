# AGENTS.md — @octocodeai/octocode-awareness

This package is the dogfood zone for shared plans, tasks, advisory file work,
exclusive sensitive locks, verification, memory, hooks, and generated repo context.

## First Move

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-codex-awareness}"
# Prefer published CLI; in this monorepo after build use the local binary:
AWARENESS="node packages/octocode-awareness/dist/bin/awareness.js"
# Elsewhere: AWARENESS="npx @octocodeai/octocode-awareness"
$AWARENESS attend --workspace "$PWD" --query "<current task>" --agent-id "$OCTOCODE_AGENT_ID" --compact
```

Follow `attend.next`. Use focused `<command> --help`, `schema json-schema <name>`,
or `docs list` only when the next action needs that contract or reference owner.

CLI order: `npx @octocodeai/octocode-awareness` (or global `octocode-awareness`);
monorepo local build `node packages/octocode-awareness/dist/bin/awareness.js`;
bundled skill fallback `node scripts/awareness.mjs` only when the package CLI is
unavailable. Install host hooks so structured edits declare presence.

## Dogfooding Contract

1. Run `attend`; inspect Ready, Claimed, Verify, and FilesUnderWork.
2. Claim a matching plan task. For independent work, run `work start` with the
   reason, files, and test plan.
3. Declare every edited file. Hooks do this for structured writes; without hooks,
   use `work start|touch` explicitly.
4. Ordinary file work is advisory: peers may work on the same file. Read their
   task/reason and coordinate when the changes interact.
5. Use `--exclusive` or `lock acquire` only for sensitive work. Exclusive
   acquisition fails while another agent has live file presence.
6. Run the declared checks, then `task submit` or `work end`, `verify mark`, and
   `verify audit`.

Standalone WORK:

```bash
$AWARENESS work start --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" \
  --file packages/octocode-awareness/<path> --rationale "<why>" \
  --test-plan "<exact check>" --compact
# edit; add files with work start --run-id <run> --file <path>, or heartbeat with work touch
$AWARENESS work end --agent-id "$OCTOCODE_AGENT_ID" --run-id <run> --compact
$AWARENESS verify mark --agent-id "$OCTOCODE_AGENT_ID" --run-id <run> \
  --message "<check result>" --compact
```

Sensitive standalone work adds `--exclusive` to `work start`. Never bypass a live
exclusive conflict; wait, signal the holder, or choose other work.

## Lifecycle

```text
ATTEND -> CHOOSE -> DECLARE -> ACT -> SUBMIT -> VERIFY -> REFLECT/HAND OFF -> MAINTAIN
```

- Plans own objectives, lead agent, members, and `.octocode/plan/**` documents.
- Tasks are the only durable selectable queue; do not create “today's tasks.”
- Runs are attempts. `run_files` records mandatory advisory path presence.
- Locks are optional and exclusive. Expiry cleans coordination; it never proves success.
- `edit_log` records completed edit events; SQLite remains canonical.

## Hooks

`pre-edit` runs the harness guard first, then extends existing WORK/TASK presence or
declares HOOK advisory presence; it blocks on live exclusive conflicts (it does not
silently acquire exclusivity). `post-edit` logs/heartbeats; task and explicit work
runs remain active, while automatic fallback runs become `PENDING`.
`notify-deliver` emits only changed briefing state. `stop-verify` caps outstanding
items and blocks/reminds. `session-end` deduplicates handoffs.

Claude may use skill frontmatter. Codex/Cursor require installed host config. Pi uses
`wirePiAwarenessHooks(pi)`. Preview config writes with `hooks install --dry-run`, then
run `hooks check --strict`.

Harness edits require user authorization, `OCTOCODE_ALLOW_HARNESS_APPLY=1`, and a
safe non-main branch. The integrated pre-edit hook performs this guard before it
writes file presence.

## Source Of Truth

Edit canonical sources:

- Runtime/CLI: `src/**`, `bin/**`, `scripts/schema.mjs`
- Skill: `skills/octocode-awareness/**`
- Package docs: `README.md`, `docs/**`
- Build: `build.mjs`

Do not hand-edit `dist/**`, `.agents/skills/**`, vendored
`skills/octocode-skills/**`, or generated `awareness.mjs`, `hook-runner.mjs`,
`extract-hook-files.mjs`, and `schema.mjs` copies. **After every source or skill
edit, rebuild before CLI/smoke/dogfood** — otherwise agents run a stale mirror:

```bash
yarn workspace @octocodeai/octocode-awareness build
```

## Documentation Owners

- `docs/HOW_IT_WORKS.md`: system architecture
- `docs/DB.md`: schema and migrations
- `docs/LOCKS.md`: advisory work, exclusivity, verification
- `docs/HOOKS.md`: host behavior
- `docs/MEMORY_NAVIGATION.md`: compact attend/workboard output
- `docs/REFLECTION.md`: learning and improvement boundary
- `docs/WIKI.md`: query/projection behavior
- Skill `SKILL.md`: compact lobby; references own one concept each
- `schema commands --compact`: command inventory

## Verification

Use the smallest focused test first, then broaden shared changes:

```bash
yarn workspace @octocodeai/octocode-awareness typecheck
yarn workspace @octocodeai/octocode-awareness test:quiet
yarn workspace @octocodeai/octocode-awareness test:smoke
yarn workspace @octocodeai/octocode-awareness verify
```

Skill changes also require the Awareness build and:

```bash
node skills/octocode-skills/scripts/skill-review.mjs \
  packages/octocode-awareness/skills/octocode-awareness
```

Preserve failed checks. Reflect only reusable outcomes. Keep one normalized
workspace, one stable agent ID, and no secrets in Awareness rows or projections.
