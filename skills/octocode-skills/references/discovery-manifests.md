# Discovery Manifests And CLIs

Load when parsing marketplace manifests or choosing an installer CLI — after `discovery-surfaces.md`.

## Manifest formats

| Format | Where | Use |
|--------|-------|-----|
| `agentskills.io/llms.txt` | Standard hub | Agent-readable doc index |
| `aiskillstore.io/llms.txt` | USK registry | Spec + endpoint catalog |
| `microsoft.github.io/skills/llms*.txt` | Microsoft | Daily catalog snapshot |
| `.claude-plugin/marketplace.json` | Marketplace repo | Anthropic marketplace |
| `.claude-plugin/plugin.json` | Plugin repo | Per-plugin manifest |
| `feed/new-skills.json` | aiskillstore | New-skill firehose |
| YAML frontmatter | Inside `SKILL.md` | `name`, `description`, optional USK fields |

## CLI installers

Always safety-scan source before install. Prefer Octocode CLI when available (`install-gates.md`).

| CLI | Pattern | Notes |
|-----|---------|-------|
| `npx skills add` | `… <gh-url> --agent <host> --skill <name>` | Symlink-by-default |
| `npx skills-installer` | `… install @owner/repo/skill --client <host>` | Pairs with claude-plugins.dev |
| `npx claude-plugins` | install/list/enable/disable | Plugin marketplace mgmt |
| Anthropic native | `/plugin marketplace add` then `/plugin install` | Claude Code |
| `npx octocode skill` | `--add --path … --platform …` | Preferred in this monorepo |

## Meta-skills

`find-skills`, `skills-discovery`, `skill-creator` — mid-session search/install helpers. Still gate writes.

Next: when ranking load `references/quality-signals.md`; if a surface fails load `references/recovery.md`; before writing load `references/install-gates.md`.
