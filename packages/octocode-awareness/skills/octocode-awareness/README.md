# Octocode Awareness Skill

This Agent Skill and the `octocode-awareness` CLI ship together in
`@octocodeai/octocode-awareness` (public CLI: `npx @octocodeai/octocode-awareness`).
In this monorepo, edit the skill at `packages/octocode-awareness/skills/octocode-awareness`; maintainers rebuild
the package after changes, while agent-facing commands still use the public runner.

The skill gives agents always-on workspace awareness: collaboration, memory, locks,
verification, hooks, reflection, and repo context. It runs a Homeostatic Awareness
Loop — sense shared SQLite/hook state, compare with bounded targets, recommend the
smallest guarded correction, re-measure. ("Living system" is a maintenance metaphor,
not autonomy.) Rationale:
[THESIS.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-awareness/docs/THESIS.md).

`SKILL.md` is the operating lobby and owns the workflow, loop, and reference routing —
read it first. This README covers only install, scripts, and hosts.

## Agent contract

The agent loop is NOTICE → SCOPE/IDENTITY → INSPECT → ACT → OBSERVE →
SETTLE/VERIFY → LEARN. Run `attend` only when shared state can change the next
action; re-observe after a material change, not on a timer. Presence, signals, and
memory are advisory evidence. Locks, schemas, and verification debt enforce their
own boundaries. An expired lease or released lock recovers coordination state; it
does not complete or verify work. `attend.next` is structured, read-first guidance,
not shell text or authorization to claim, edit, compact, or mark a check.

## Export agent instructions

The package can emit its maintained instruction fragment without reading or copying
`SKILL.md` from a prompt:

```bash
npx @octocodeai/octocode-awareness instructions export --format prompt
npx @octocodeai/octocode-awareness instructions export --format agents-md
npx @octocodeai/octocode-awareness instructions export --format json
```

Use `prompt` for dynamic system/developer prompt composition. Use `agents-md` for an
`AGENTS.md` block; its stable start/end comments let a host replace the existing
block idempotently. Output goes only to stdout, so the caller retains control over
file writes. The full installed skill supplies progressive detail; this export is
the concise activation, discovery, coordination, and safety contract.

## Initialize

```bash
npx @octocodeai/octocode-awareness attend --workspace "$PWD" --compact
```

Explicit CLI use needs no global feature configuration. When enabling shell-hook
automation, run `config show`; if configuration is missing, ask all five feature
questions together, create `$OCTOCODE_HOME/awareness.json` with `config init`, and
validate it. Configuration preferences never authorize hook installation.

For the optional advanced workflow store:

```bash
npx @octocodeai/octocode-awareness maintenance init --compact
```

Install this bundled skill through the public CLI. Choose an explicit platform and
scope, preview the destination, then rerun without `--dry-run` only after approval:

```bash
npx @octocodeai/octocode-awareness skill install --platform shared --project-dir "$PWD" --dry-run
```

Run `npx @octocodeai/octocode-awareness skill install --help` for user-level and
host-specific destinations. The CLI copies its packaged skill directly; do not
reconstruct package paths in an agent prompt. `maintenance init` is safe to repeat.

Awareness is the package's only bundled skill. The separately owned
`octocode-orchestrator` skill remains in the sibling
[`octocode-agent` repository](https://github.com/bgauryy/octocode-agent/tree/main/skills/octocode-orchestrator).
Install other workflow skills with `octocode skill install <name>` when needed.

Discovery is lazy — reach for an inventory only when the next action needs it:

```bash
npx @octocodeai/octocode-awareness schema commands --compact
npx @octocodeai/octocode-awareness docs list --compact
```

## Scripts

| Script | Purpose |
|---|---|
| `scripts/awareness.mjs` | Bundled CLI/runtime; serves every `schema` contract dynamically. |
| `scripts/hook-runner.mjs` | Shared host lifecycle implementation. |
| `scripts/extract-hook-files.mjs` | Host payload path extraction. |
| `scripts/hooks/*.sh` | Thin lifecycle wrappers. |

`agents/openai.yaml` supplies the OpenAI skill interface metadata.
`evals/trigger-cases.json` is the maintained activation regression corpus.

These are generated artifacts — do not hand-edit. Maintainers regenerate them from
`src/schema/*.ts` and `bin/*.ts`.

## Hosts

- Claude may run frontmatter hooks while the skill is active.
- Codex/Cursor: preview `npx @octocodeai/octocode-awareness hooks install --host <codex|cursor> --project-dir "$PWD" --dry-run`, request approval immediately before mutation, install only after explicit approval, then run `npx @octocodeai/octocode-awareness hooks check --host <codex|cursor> --project-dir "$PWD" --strict`.
- Pi uses native `@octocodeai/pi-extension` events; never run `hooks install --host pi`.
- Normal hooks are silent; only changed peers/briefings and real conflicts surface.

## Verification (monorepo)

```bash
yarn workspace @octocodeai/octocode-awareness build
yarn workspace @octocodeai/octocode-awareness test:quiet
```

Build emits `out/octocode-awareness.js`, then mirrors this skill to package
`out/skills/` and local `.agents/skills/`. For native host integration changes,
also run the relevant checks in the sibling `octocode-agent` repository.
