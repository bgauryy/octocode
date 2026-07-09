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

## Skills (install / update / lint)

Sibling skill `octocode-skills` ships next to this skill in the awareness package bundle. Use `npx octocode` for skill install/update/lint and for Octocode research/search operations — gate every write.

```bash
# Install / refresh both bundled skills for a host; use common for ~/.agents/skills
npx octocode skill --add --path "<awareness-package>/dist/skills/octocode-awareness" --platform codex --force
npx octocode skill --add --path "<awareness-package>/dist/skills/octocode-skills" --platform codex --force

# Initialize store and smoke the CLI
<cli> maintenance init --compact
<cli> attend --workspace "$PWD" --query "smoke" --compact

# Hooks: preview first, install after approval, then verify host wiring
<cli> hooks install --host codex --project-dir "$PWD" --dry-run --compact
<cli> hooks install --host codex --project-dir "$PWD" --compact
<cli> hooks check --host codex --project-dir "$PWD" --strict --compact
```

Load `octocode-skills` when the job is skill discovery/install/review; keep using this skill for workspace awareness. Do not install `octocode-awareness` by registry name: the `@octocodeai/octocode-awareness` package already bundles the canonical skill.

## Code search (not bundled here)

```bash
npx octocode search <dir> --tree --max-depth 2 --no-color
npx octocode search "<term>" <path> --no-color
npx octocode search <file> --content-view exact --no-color
```

Use `npx octocode` so the platform-native engine resolves correctly. Details: `references/octocode.md`.
