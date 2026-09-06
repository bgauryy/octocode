# Verify Octocode Awareness

Use this runbook when any agent must answer: “Is Octocode Awareness working?” It
owns the verification sequence and receipt shape; command schemas remain owned by
the CLI, hook semantics by [HOOKS.md](HOOKS.md), and invariants by
[HARNESS.md](HARNESS.md).
## Choose Scope

| Scope | Use when | Required result |
|---|---|---|
| Quick | Confirm an installed CLI/store can start | Self-test, schema, status, attend |
| Installed | Validate the shipped skill bundle and coordination runtime | Quick + install diagnostic + multi-agent smoke + [full feature sweep](FEATURE_SWEEP.md) |
| Host | Rely on supported host automation | Installed + config check + observed runtime event |
| Monorepo | Change package source, docs, hooks, or skill | Full build/test/smoke/review matrix |
| Release | Publish or validate an npm artifact | Monorepo + pack check |
| Complete | All | [audit](COMPREHENSIVE_AUDIT.md) |
| Interview | Multiple agents must judge agent-experience or coordination-health questions | Quick + independent panel + synthesized scorecard ([Peer Interview](#peer-interview)) |

Run only the lanes in scope. Never report “all working” when a required lane was
skipped or blocked.
## Safety

- Start with non-source-mutating checks. They may initialize or update Awareness
  metadata, but must not edit repository source.
- Use a temporary workspace/database for collision, lock, cleanup, and hook-install
  tests. Do not pollute a user’s real workboard.
- Use `--dry-run` before any hook config write. Get user approval before changing a
  real project or global host config.
- Skill frontmatter and project settings may be alternative hook surfaces; do not
  install duplicate definitions for the same host.
- Preserve exact failing output and exit code. Do not convert unavailable tooling
  into a product failure.
- Pass `--workspace` on path-resolution commands; a bare relative path can resolve
  against the wrong root and false-pass while a peer still holds the file.

```bash
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-awareness-check}"
```
## Quick Check

Run from the target repository:

```bash
npx @octocodeai/octocode-awareness maintenance self-test --compact
npx @octocodeai/octocode-awareness schema commands --compact
npx @octocodeai/octocode-awareness status --workspace "$PWD" --compact
npx @octocodeai/octocode-awareness attend --workspace "$PWD" --query "verify Awareness health" \
  --agent-id "$OCTOCODE_AGENT_ID" --compact
```

Pass when self-test returns `ok:true`, schema discovery returns commands, workspace
status opens the intended store/scope, and attend returns `ok:true` plus an
actionable `next`. Follow `attend.next` only when it is relevant to the check; do not
drain unrelated inbox or maintenance work.
## Installed Bundle Check

Run the public package diagnostics without resolving or invoking package-internal
scripts:

```bash
npx @octocodeai/octocode-awareness maintenance self-test --compact
npx @octocodeai/octocode-awareness schema commands --all --compact
npx @octocodeai/octocode-awareness schema entities --compact
npx @octocodeai/octocode-awareness status --workspace "$PWD" --compact
```

The diagnostics must report healthy stores and live catalogs. The feature sweep must
prove advisory overlap, exclusive conflict, pending verification, verify clearance,
message delivery, stale-lock cleanup, and a zero-debt final audit.
## Full Feature Sweep

Run the isolated [full feature sweep](FEATURE_SWEEP.md) for planning, learning,
docs catalog, registry, and maintenance surfaces. Every listed pass signal is required;
an errored, missing, or wrong-workspace step makes Installed scope FAIL.

## Host Hook Check

Config check is read-only:

```bash
npx @octocodeai/octocode-awareness hooks check --host <claude|codex|copilot|cursor|gemini|opencode> \
  --project-dir . --strict --compact
```

Pass config only when all expected entries are present with zero missing/drifted
items. `health.config:ready` does not prove execution: `health.runtime` remains
`unverified` until a real host event is observed.

If config is missing and installation is in scope, preview first. Apply only after
approval for a real project:

```bash
npx @octocodeai/octocode-awareness hooks install --host <claude|codex|copilot|cursor|gemini|opencode> \
  --project-dir . --dry-run
# after review and approval:
npx @octocodeai/octocode-awareness hooks install --host <claude|codex|copilot|cursor|gemini|opencode> \
  --project-dir . --compact
npx @octocodeai/octocode-awareness hooks check --host <claude|codex|copilot|cursor|gemini|opencode> \
  --project-dir . --strict --compact
```

Then make one approved harmless structured edit and observe the complete edge:

1. Pre-edit declares path presence or blocks a real exclusive conflict.
2. Post-edit records/heartbeats without ending explicit WORK/TASK.
3. Stop or host end exposes fallback verification debt.
4. The declared check plus `verify mark` clears `verify audit`.
5. PreCompact keeps the session reusable; SessionEnd marks it ended.

Verify Claude, Codex, and Cursor hooks on the host that executes them: pre-edit
guard/presence, successful and failed write handling, prompt briefing, compaction,
and stop/session behavior.
## Full Monorepo Check

Run from the monorepo root. Keep the explicit matrix: the package `verify` script
does not replace artifact, smoke, or focused skill-behavior checks.

```bash
yarn workspace @octocodeai/octocode-awareness build
yarn workspace @octocodeai/octocode-awareness lint
yarn workspace @octocodeai/octocode-awareness typecheck
yarn workspace @octocodeai/octocode-awareness test
yarn workspace @octocodeai/octocode-awareness test:smoke
npx @octocodeai/octocode-awareness maintenance self-test --compact
```

Pass when every command exits `0`, coverage thresholds pass, focused skill checks
pass, and smoke ends with zero verification debt or active locks. When a change
affects native host integration, also run the relevant checks in the sibling
[`octocode-agent` repository](https://github.com/bgauryy/octocode-agent/tree/main/packages/octocode-agent).
## Release Add-On

```bash
yarn workspace @octocodeai/octocode-awareness pack:check
```

Pass when Yarn's isolated packed artifact loads its CLI, schemas, and library
entrypoint with only the intended publish surface.
## Peer Interview

Use when the question is “how do agents judge a capability,” not “does a command pass or fail” — auditing Awareness itself or any multi-agent coordination question. One agent is the **interviewer**; it spawns independent **panelists** with no shared context, each required to cite a command it ran for every claim. Quick-Check scope only: read-only, no hook install, no lock/work/task claims, no source mutation.

**Algorithm:** (1) interviewer picks 3-5 lenses that would disagree if a real gap existed — default panel: onboarding, operations, reliability, memory/context, skepticism; (2) spawn panelists independently with the question, lens, read-only commands, and the evidence rules below, no shared context; (3) every claim cites the command + field observed; (4) interviewer collects one report per lens and does not average away disagreement — differing readings of the same metric minutes apart mean the database changed between reads, report both with timestamps; (5) interviewer synthesizes a scorecard vs. any prior interview's open items (shipped/partial/open, with evidence), one verdict per lens in the panelist's own words, and one interviewer verdict.

**Evidence rules:** live state is a snapshot, not a fact; declared presence (`work list`, `attend.FilesUnderWork`) is intent, not edit-proof; `verify mark`/`verify audit` are claims, not independent proof; `hooks check` config `ready` never implies `runtime` execution (stays `unverified` until a real host event); opinions need a controlled comparison to be more than opinion.

**Report shape:** `## <Lens> agent — <topic>`, Q&A citing commands, an evidence table, one-line verdict. Interview scope has no PASS/FAIL/BLOCKED — its receipt is this scorecard, not the record below.
## Verdict

- **PASS** — every required lane ran and its pass signal was observed.
- **FAIL** — a required product behavior or assertion failed; preserve evidence.
- **BLOCKED** — a prerequisite or approval is unavailable; name the exact lane and
  next command. Other passing lanes remain valid but cannot justify “all working.”
## Receipt

Return this compact record:

```text
Awareness verification: PASS | FAIL | BLOCKED
Scope: quick | installed | host:<name> | monorepo | release
Runtime: node=<version> awareness=<version/path> workspace=<path>
Checks: <command/lane>=<exit + decisive signal>; ...
Hooks: config=<ready|missing|drifted|n/a> runtime=<observed|unverified|n/a>
Debt: pending=<count> active=<count> locks=<count>
Blocked/skipped: <none or exact prerequisite + next action>
Evidence: <test counts, coverage, smoke receipt, relevant paths>
```

Close any explicit verification WORK only after its checks run, then `verify mark`
and `verify audit`. Record reusable failures only; do not publish routine check logs
to memory or `.octocode/` query exports.
