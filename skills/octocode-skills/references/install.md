# Install, fetch, and sync

Load when you install a skill, install or adapt a remote skill into a local folder, or symlink a local skill into vendor skill dirs. Why: confirm source, destination, and authority before any write; wrong scope hides the skill or pollutes every project.

An install copies or symlinks a `SKILL.md` folder into a path the runtime scans.

1. Normalize the source: `owner/repo/path`, GitHub tree/blob URL, or a local path. Strip a trailing `SKILL.md`. The name is the final folder segment. If frontmatter `name` differs, surface it and ask.
2. Resolve from the request and session; ask only for missing choices: providers, scope per provider (user / project / custom), project root, mode.
3. Read third-party scripts and hooks. This read needs no separate approval.
4. Per destination, run `ls "<dest>/<skill-name>"`. On conflict choose Overwrite / Skip / Rename / Diff / Cancel. Never overwrite silently.
5. Write after authority covers the plan: `npx -y octocode skill install --add <src> --platform <hosts> [--mode copy|symlink|hybrid]`. It copies to `<octocode-home>/skills/<name>` and links vendors from that copy; `--mode` changes vendor destinations only.
6. Verify `test -f <dest>/<skill-name>/SKILL.md`; give a reload hint.

Symlink only a stable local source that the user edits live. Else copy.

## Destinations

| Provider | User | Project |
|---|---|---|
| claude (`claude-desktop`) | `<home>/.claude/skills/` | `<repo>/.claude/skills/` |
| cursor | `<home>/.cursor/skills/` | `<repo>/.cursor/skills/` |
| codex (`agents`, `shared`, `common`, `codex-native`) | `<home>/.agents/skills/` | `<repo>/.agents/skills/` |
| opencode | `<home>/.config/opencode/skills/` | `<repo>/.opencode/skills/` |
| pi | `<home>/.pi/agent/skills/` | `<repo>/.pi/skills/` |
| copilot | `<home>/.copilot/skills/` | `<repo>/.github/skills/` |
| gemini | `<home>/.gemini/skills/` | `<repo>/.gemini/skills/` |
| other | path the runtime scans | confirmed in-repository path |

Project scope fits repository-specific skills; user scope fits general ones. Treat an unknown provider as a custom path and confirm the runtime scans it.

## Fetch a remote skill

Fetch → scan → gate before any destination write.

1. Confirm intent: verbatim install or adapt.
2. Normalize the source and resolve destinations (steps 1–2 above).
3. Inspect through `octocode-research`, then fetch: `npx -y octocode cache fetch owner/repo path --depth clone` (add `--branch <ref>`; omit `path` for the whole repository). The result stays in the Octocode cache.
4. Validate: the folder has `SKILL.md` with `name` + `description`.
5. Safety-scan `SKILL.md`, `scripts/`, and hooks (`references/hooks.md`); flag risk before any write.
6. To adapt, follow `references/skill-authoring.md` § Create a local skill. Reuse only license-allowed patterns and cite the source.
7. Check conflicts per destination and apply the user's choice.
8. Write by copy only (never symlink a fetch). Verify `test -f <dest>/SKILL.md`.
9. Report each destination result and how the runtime reloads skills.

Never write fetched scripts or hooks silently, rename silently, or copy wholesale without a license and user approval. Surface missing or restrictive licenses. Partial download: re-fetch once, then stop. Intent flip mid-flow: keep the cache and resume from step 5 or 6.

## Sync a local skill to vendors

Use `scripts/skill-sync.mjs` for a stable local source you control (dogfood or live edits), or when the user asks to sync or symlink. For published skills, prefer `npx -y octocode skill install --add …`. Never symlink a temporary fetch. The default run is a dry-run plan. Read the plan before `--approve`. Existing authority can cover source, destinations, and conflict policy; ask only for authority not yet granted.

```bash
node scripts/skill-sync.mjs <skill-dir> --platforms top
node scripts/skill-sync.mjs <skill-dir> --platforms top --approve
node scripts/skill-sync.mjs <skill-dir> --platforms claude,cursor --approve --force
node scripts/skill-sync.mjs --list-vendors
```

- `--force` replaces conflicts and requires `--approve`. The script never prompts.
- `top` = `claude`, `cursor`, `codex` (`<home>/.agents/skills`). `all` adds `opencode`, `pi`, `copilot`, `gemini`.
- Aliases: `agents`, `shared`, `common`, `codex-native` → `codex`; `claude-desktop` → `claude`.
- The script mirrors the shared installer registry (CI contract-tests the mirror) because an installed skill cannot import workspace packages.

## Recovery

- Missing parent dir: create it after approval; do not auto-create deep custom trees.
- Permission denied: report the path; offer another scope.
- Partial multi-target: report per destination; do not roll back others without asking.
- Invalid frontmatter: do not install.
- 404 or permission errors on fetch: load `references/recovery.md`.
