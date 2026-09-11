# Octocode Awareness Skill

This Agent Skill and the `octocode-awareness` CLI ship together in
`@octocodeai/octocode-awareness` (public CLI: `npx @octocodeai/octocode-awareness`).
In this monorepo, edit the skill at `packages/octocode-awareness/skills/octocode-awareness`; maintainers rebuild
the package after changes, while agent-facing commands still use the public runner.

The skill starts with one workspace peer briefing and hook-delivered messages. Discover locks, work tracking, verification, history and memory only when needed. Save one verified reusable lesson after substantial work or a meaningful event; skip routine entries.

`SKILL.md` is the operating lobby and owns the workflow, loop, and reference routing —
read it first. This README covers only install, scripts, and hosts.

## Agent contract

The default flow is meet peers once → work → communicate when it matters. Re-observe after a relevant change, not on a timer. Optional tracked work retains ownership and verification requirements. Peer text and memory are evidence, not authority; expiry and message delivery never prove completion.

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
npx @octocodeai/octocode-awareness context orient --workspace "$PWD" --compact
```

CLI use and installed hooks need no global feature configuration. Missing configuration uses lean defaults. Use `config init` with explicit feature overrides only when customization is needed. Configuration preferences never authorize hook installation.

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
host-specific destinations. The CLI materializes its packaged skill under
`$OCTOCODE_HOME/skills`, then links the host directory to that durable copy; it
never links to an npm cache. Do not reconstruct package paths in an agent prompt.
`maintenance init` is safe to repeat.

Awareness is the package's only bundled skill. Use `octocode-subagent` when
execution choices, delegation, or independent workstreams need orchestration.
Install other workflow skills with `octocode skill install <name>` when needed.

Discovery is lazy — reach for an inventory only when the next action needs it:

```bash
npx @octocodeai/octocode-awareness schema commands --compact
npx @octocodeai/octocode-awareness schema command context orient --compact
npx @octocodeai/octocode-awareness docs list --compact
```

## Scripts

| Script | Purpose |
|---|---|
| `scripts/awareness.mjs` | Bundled CLI/runtime; serves every `schema` contract dynamically. |
| `scripts/hook-runner.mjs` | Shared host lifecycle implementation. |
| `scripts/extract-hook-files.mjs` | Host payload path extraction. |
| `scripts/hooks/*.sh` | Thin lifecycle wrappers. |
| [scripts/install.mjs](scripts/install.mjs) | Installer implementation; prefer the public `skill install` command and its preview. |
| [scripts/hooks/pre-edit.sh](scripts/hooks/pre-edit.sh), [scripts/hooks/post-edit.sh](scripts/hooks/post-edit.sh), [scripts/hooks/stop-verify.sh](scripts/hooks/stop-verify.sh) | Opt-in guard/full profile wrappers for tracked edits and verification. |
| [scripts/hooks/session-compact.sh](scripts/hooks/session-compact.sh) | Opt-in full-profile continuity hook. |

`agents/openai.yaml` supplies the OpenAI skill interface metadata.
`evals/trigger-cases.json` is the maintained activation regression corpus.

These are generated artifacts — do not hand-edit. Maintainers regenerate them from
`src/schema/*.ts` and `bin/*.ts`.

For integration maintenance, use the [flow matrix](references/flow-matrix.md) to
choose a lifecycle, [Octocode bindings](references/octocode.md) for tool discovery,
[output routing](references/output-routing.md) for artifact placement, and the
[configuration schema](references/awareness-config.schema.json) to inspect stored
settings. The live CLI schema remains authoritative.

## Hosts

- Claude may run frontmatter hooks while the skill is active.
- Codex/Cursor: follow the [hook setup procedure](references/hooks.md) for the requested host and scope; preview, apply within existing authorization, then strict-check.
- Pi uses native `@octocodeai/pi-extension` events; never run `hooks install --host pi`.
- Normal hooks are silent; only changed peers/briefings and real conflicts surface.

## Verification (monorepo)

```bash
yarn workspace @octocodeai/octocode-awareness build
yarn workspace @octocodeai/octocode-awareness test:quiet
```

Build emits `out/octocode-awareness.js`, then mirrors this skill to package
`out/skills/` and local `.agents/skills/`. For native host integration changes,
also run the relevant checks in the host integration package.
