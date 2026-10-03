# Frontmatter and hosts

Load when you write or check `SKILL.md` frontmatter, use a host-only field, or target a host. Why: the spec is the portable floor; other hosts ignore, warn on, or reject host extras.

## Spec fields (agentskills.io/specification)

| Field | Required | Rule |
|---|---|---|
| `name` | yes | 1–64 chars; `a-z`, `0-9`, `-`; no leading, trailing, or `--` hyphen; equals the folder name |
| `description` | yes | 1–1024 chars; what the skill does and when to use it; trigger keywords |
| `license` | no | short: a license name or a bundled license file |
| `compatibility` | no | 1–500 chars; only for real requirements: product, system packages, network |
| `metadata` | no | string → string map; use unique keys (for example `author`, `version`) |
| `allowed-tools` | no | experimental; space-separated pre-approved tools, for example `Bash(git:*) Read` |

Regex for `name`: `^[a-z0-9]+(-[a-z0-9]+)*$`. Anthropic suggests gerund names (`processing-pdfs`); this is advice, not spec. House rule: octocode skills use `octocode-<noun>` so a family sorts together. Validate with the spec tool `skills-ref validate ./my-skill` or with `skill-review`.

Anthropic surfaces (claude.ai upload, Skills API, `package_skill.py`) also reject XML tags in `name` or `description` and any field beyond the six above. Octocode house rules that are stricter than the spec live in the lobby ACT rules.

## Host extras

| Host | Extra fields and body features | Unknown fields |
|---|---|---|
| Claude Code | `when_to_use`, `argument-hint`, `arguments`, `disable-model-invocation`, `user-invocable`, `allowed-tools`, `disallowed-tools`, `model`, `effort`, `context: fork` + `agent` + `background`, `paths`, `hooks`, `shell`; body: `$ARGUMENTS`, `$0`, `${CLAUDE_SKILL_DIR}`, inline shell-command injection | accepted locally; rejected on claude.ai upload |
| Codex | `agents/openai.yaml`: `interface` (display name, `short_description`, icons), `policy.allow_implicit_invocation`, `dependencies.tools` (MCP servers) | not documented |
| Cursor | `paths`, `disable-model-invocation`, `icon`, `color` | not documented |
| OpenCode | none; `permission.skill` in `opencode.json` allows, denies, or asks per skill | ignored |
| Pi | `disable-model-invocation`; `/skill:name` commands | warning, still loads |

Keep host extras out of skills you publish to several hosts. When you need a host field, name the host in `compatibility`.

## Budgets and collisions

| Host | Listing budget | Same name twice |
|---|---|---|
| Claude Code | `description` + `when_to_use` cut at 1,536 chars | enterprise > personal > project; nested skills both load |
| Codex | list ≤2% of context (8,000 chars if unknown); descriptions shorten first | both show; no merge |
| Cursor | — | nested `.cursor/skills/` scope to their subtree |
| OpenCode | — | walks up to the git worktree |
| Pi | — | first discovered wins, with a warning |

Each host also scans the shared `.agents/skills/` path (Claude Code excepted) and often `.claude/skills/`. One folder linked into each host dir avoids drift; install destinations live in `references/install.md`.

Next: to tune the trigger load `references/description-tuning.md`; before done load `references/skill-review.md`.
