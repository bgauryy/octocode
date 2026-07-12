# Agent Cheat Sheet — Agents, Skills & Search
Core loop: `references/agent-cheatsheet.md`. Finish/handoffs: `references/agent-cheatsheet-finish.md`.

## Agents + docs
```bash
<cli> agent register --agent-id "$OCTOCODE_AGENT_ID" --agent-name "<host>" --workspace "$PWD" --compact
<cli> agent list --workspace "$PWD" --compact
# only when the needed reference owner is unknown:
<cli> docs list --compact
<cli> docs show architecture
<cli> docs staleness --targets-json '[{"docFile":"README.md","sourceDirs":["src"]}]' --compact
```
`docs list --compact` returns name/title routing only; `docs show` loads one skill
reference. Neither indexes package `docs/**`.

## Skills (install / update)
Official Octocode skills are Awareness (required) and Research (optional). Use `npx octocode` for skill install and for Octocode research/search operations — gate every write.
```bash
# `common` means ~/.agents/skills; use claude/cursor/codex/pi for a host-specific destination.
npm install --global @octocodeai/octocode-awareness
npx octocode skill --add --path "$(npm root --global)/@octocodeai/octocode-awareness/out/skills/octocode-awareness" --platform common --dry-run
# after reviewing destinations and approving the write:
npx octocode skill --add --path "$(npm root --global)/@octocodeai/octocode-awareness/out/skills/octocode-awareness" --platform common --force
npx octocode skill --add --path "$(npm root --global)/@octocodeai/octocode-awareness/out/skills/octocode-research" --platform common --force
# see `<cli> --help` or install.mjs's bundled_skills for the current list
# Initialize store and smoke the CLI
export OCTOCODE_AGENT_ID="${OCTOCODE_AGENT_ID:-my-agent}"
<cli> maintenance init --compact
<cli> attend --workspace "$PWD" --query "smoke" --agent-id "$OCTOCODE_AGENT_ID" --compact

# Codex/Cursor: preview first, install after approval, then verify host wiring.
<cli> hooks install --host <codex|cursor> --project-dir "$PWD" --dry-run
<cli> hooks install --host <codex|cursor> --project-dir "$PWD" --compact
<cli> hooks check --host <codex|cursor> --project-dir "$PWD" --strict
```
Noncompact dry-run/check exposes settings and runtime-health detail; compact output is only a receipt.
Claude skill frontmatter is already a hook surface; do not also install project
settings. Use `--host claude` only when frontmatter is unsupported or disabled.

Do not install `octocode-awareness` by registry name: the `@octocodeai/octocode-awareness` package already bundles the canonical skill.
## Code search (not bundled here)

```bash
npx octocode search <dir> --tree --max-depth 2 --no-color
npx octocode search "<term>" <path> --no-color
npx octocode search <file> --content-view exact --no-color
```

Use `npx octocode` so the platform-native engine resolves correctly. Details: `references/octocode.md`.
