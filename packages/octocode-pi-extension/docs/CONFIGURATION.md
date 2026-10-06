# Configuration

Where Octocode reads config and writes state, what it trusts, and every environment variable. See the [README](../README.md) for an overview and [FEATURES.md](FEATURES.md) for behavior.

## Where files go

`.octocode` is the one place Octocode reads its own config and saves things, at two levels: the Octocode home (`OCTOCODE_HOME`, default `~/.octocode`: `skills/`, `agents/`, `hooks.json`, runtime state) and the workspace `<repo root>/.octocode` (`skills/`, `agents/`, `hooks.json`, trusted projects only; `tmp/` scratch for agent hand-offs). Workspace paths resolve from the repository root, so a session started in a subfolder reads the same files. Everything the extension writes at runtime lives under the workspace or the Octocode home (`OCTOCODE_HOME`, default `~/.octocode`). Per-session state lives in `agent/pi/`: the agent database `octocode.db` (session index, backlog, memories; `OCTOCODE_AGENT_DB` moves it) and one folder per session, `sessions/<session id>/`, holding the full text of oversized `web`, `browser` and bash results (`output/`, readable by the Octocode MCP tools even when `OCTOCODE_HOME` is outside the home directory), bash job logs and truncated bash output (`bash/`), and edit checkpoints (`checkpoints/`); output written before a session id is known goes to `sessions/_pid-<pid>/`. The rest: browser profiles (`pi-browser/` temporary, `pi-browser-profile/` for `webLive`), subagent screenshots (`pi-team/shots/`), worktrees (`pi-worktrees/`), and API records (`pi-api/`); `browser download` saves to `<workspace>/downloads/` and `/octocode backlog export` to `<repo>/.octocode/backlog.md`. The one exception is an API Unix socket whose path under the home would exceed the OS limit of about 100 bytes: it falls back to the private temp directory.

## Environment variables

| Variable | Effect |
|----------|--------|
| `OCTOCODE_HOME` | The Octocode home (default `~/.octocode`): user config (`skills/`, `agents/`, `hooks.json`) and everything the extension writes at runtime |
| `OCTOCODE_AGENT_DB` | Agent database path: session extras, backlog, memories and team tables (default `<Octocode home>/agent/pi/octocode.db`) |
| `OCTOCODE_CLEANUP_DAYS` | Days spilled output and bash logs are kept in a session folder (default 30); `0` turns the session sweep off |
| `OCTOCODE_MEMORY_AUTO=0` | Do not inject memories before prompts in this process (`/octocode memory auto off` turns it off everywhere) |
| `OCTOCODE_EXTRA_SKILLS=0` | Skip the extra skill directories (`~/.claude/skills` without its claude.ai `synced/` folder, `~/.codex/skills`, `<Octocode home>/skills`, project `.claude/skills` and `.octocode/skills`) |
| `TAVILY_API_KEY` / `SERPER_API_KEY` / `EXA_API_KEY` / `BRAVE_API_KEY` | Web search providers, tried in this order before the keyless pages (DuckDuckGo, then Bing, then DuckDuckGo lite); every provider passes to the next when it fails or finds nothing |
| `OCTOCODE_WEB_LANGUAGE` | Accept-Language that `web` sends (default `en-US,en;q=0.9`) |
| `OCTOCODE_BROWSER_LOCALE` / `OCTOCODE_BROWSER_TIMEZONE` | Pin the browser's language (for example `en-US`: `navigator.language`, `Intl` formatting, Accept-Language) and time zone (for example `America/New_York`). Sites that price by IP address still answer for the machine's location |
| `OCTOCODE_CHROME_PORT` | Opt in to sharing an open Chrome: the browser tool opens (and later closes) its own tab on this remote-debugging port. Unset, it never attaches on its own; a headless Chrome is launched when nothing listens |
| `OCTOCODE_BROWSER_HEADLESS=0` | Show the launched Chrome window (temporary profile) |
| `OCTOCODE_BROWSER_VISIBLE=1` | Visible Chrome on the persistent profile `<Octocode home>/pi-browser-profile/` (set by `webLive`) |
| `OCTOCODE_API` | `1` serves the [External API](API.md) on a Unix socket (off) |
| `OCTOCODE_API_HTTP` | Also serve the API over loopback HTTP on this port (`0` = any free port) |
| `OCTOCODE_API_DIR` | API instance-record directory (default `<Octocode home>/pi-api`) |
| `OCTOCODE_MCP=0` | Do not register the built-in `octocode` server (set for profiles with `mcp: false`) |
| `OCTOCODE_MCP_DIRECT=1` | Declare all nine Octocode MCP tools directly; by default the GitHub and npm tools load through `tool_search` |
| `OCTOCODE_BASH_TIMEOUT` | Longest wait for a foreground bash command, in seconds (default `900`) |
| `OCTOCODE_MAX_SUBAGENTS` | Subagents allowed to run at once, foreground and background together (default `3`) |
| `OCTOCODE_SUBAGENT_COLLABORATE` | `1` makes `agent` calls collaborate by default (off) |
| `OCTOCODE_SUBAGENT_IDLE_MINUTES` | Stop a subagent that emits no output for this many minutes while its model generates (default `10`; `0` = never). A running tool or the settle after its answer gets 15 more minutes |
| `OCTOCODE_SUBAGENT_MAX_MINUTES` | Wall-clock limit for one subagent run, in minutes (default `0` = none) |
| `OCTOCODE_LEASE_IDLE_MINUTES` | Minutes an idle session keeps renewing its file reservations (default `30`; `0` = until it ends) |
| `OCTOCODE_TEAM_WORKSPACE` | Absolute path used as the team scope instead of the git root (set for isolated worktree subagents) |
| `OCTOCODE_HOOKS` | `1` runs Claude Code / Codex command hooks (off) |
| `OCTOCODE_REVIEW` | `1` starts with file review on (off) |
| `OCTOCODE_NOTIFY` | When to signal a finished answer or an open dialog: `unfocused` (default: only while the terminal is in the background), `always` or `off` |
| `OCTOCODE_NOTIFY_METHOD` | `osc9`, `osc777`, `osc99` or `bel` (default: picked from the terminal: OSC 99 for Kitty, OSC 9 for iTerm2, OSC 777 for Ghostty and WezTerm, else the bell; wrapped for tmux) |
| `OCTOCODE_WEB_ALLOW_PRIVATE=1` | Let `web` fetch, and `browser` open without asking, loopback, private and link-local addresses (blocked by default) |

## Trust

Project hooks (`.claude/settings.json`, `.claude/settings.local.json`, `.codex/hooks.json`, `.octocode/hooks.json`), subagent profiles (`.pi/agents`, `.octocode/agents`) and skills (`.claude/skills`, `.octocode/skills`) can start commands or steer the agent, so they need an explicit trust decision. Project MCP files are Pi's (`.pi/mcp.json`) and gated by Pi's own trust. Pi's own trust is not enough on its own: Pi only asks when a project holds Pi resources (`.pi/settings.json`, `.pi/extensions`, `.agents/skills`, …) and otherwise reports it trusted. So these files load only after `/octocode trust`, which lists exactly what would run (every hook command, never cut; agent and skill files, the skill files being every file Pi's skill loader reads: root `.md` files, a root `SKILL.md` and `SKILL.md` at any depth), asks, and reloads. The listing is fingerprinted before the dialog; if the files change while it is open, it asks again about the new content. When Pi itself resolved trust for such a project (or a saved `/trust` applies) and Octocode has no decision yet, the gated files present at that point (possibly none) are recorded as trusted without a dialog. Either way the decision is stored in `<Octocode home>/pi-trust.json` (`0600`), one per repository root (so it holds in any subfolder and through symlinked paths), with a SHA-256 of those files, so any change to them (such as a later-pulled hook file) needs `/octocode trust` again, in a Pi-trusted project too; `/octocode trust off` withdraws it, also in a Pi-trusted project. The fingerprint covers the gated config files themselves (hook JSON, agent `.md` files, skill files), not the scripts or binaries their commands run: a later change to, say, `./scripts/hook.sh` is not re-prompted, so trust a project only when you trust what its commands point at. Until then startup skips them with one notice (startup never opens a dialog; print and JSON runs just skip). Subagents follow the parent session's decision; an isolated (worktree) subagent, whose checkout is a different root, follows the decision stored for the parent's repository (passed as `OCTOCODE_TRUST_ROOT`), and only while its gated files are byte-for-byte the ones that decision covered; otherwise they stay off, since no one can be asked in a subagent. A project Pi was told not to trust (`--no-approve`, a declined prompt) never loads them.

## Hooks

**Quick start.** Hooks are off by default because they run arbitrary commands. Start Pi with `OCTOCODE_HOOKS=1 pi`, then put Claude Code / Codex-style hooks in `~/.claude/settings.json` or `~/.codex/hooks.json` or `~/.octocode/hooks.json` (all projects), or `<repo>/.claude/settings.json`, `.claude/settings.local.json`, `.codex/hooks.json` or `.octocode/hooks.json` (this repository, trusted projects only). All use the same `{ "hooks": { "PreToolUse": [...] } }` format. Existing Claude Code hooks work as they are. Type `/hooks` to see whether hooks are on, what loaded, and which files exist (✓). Files are read at session start, so run `/reload` after editing.

```json
{
  "hooks": {
    "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "./scripts/check-bash.sh", "timeout": 30 }] }],
    "PostToolUse": [{ "matcher": "Edit|Write", "hooks": [{ "type": "command", "command": "yarn lint --quiet" }] }]
  }
}
```

Only `type: "command"` hooks for `PreToolUse`, `PostToolUse`, `SessionStart`, `PreCompact`, `Stop` and `Notification` are used; other events are ignored. As in Claude Code, all matching hooks run in parallel and a command listed in several files runs once. `timeout` is in seconds (default 60, at most 600). The hook gets the event as JSON on stdin and `CLAUDE_PROJECT_DIR` set to the repository root. See [Hooks in FEATURES.md](FEATURES.md#hooks-opt-in) for how each event maps to Pi.

## Subagent profiles

Profiles are Markdown files with frontmatter. Bundled profiles live in `subagents/`; add your own in `~/.pi/agent/agents/`, `~/.octocode/agents/`, `<repo root>/.pi/agents/` or `<repo root>/.octocode/agents/` (the last two in trusted projects only). Later directories override earlier ones by name, in that order, after the bundled ones.

```markdown
---
name: migrator
description: Applies a database migration and verifies it
tools: read,edit,write,bash
model: anthropic/claude-sonnet-4-5
---
Instructions for the subagent…
```

`tools` is an allowlist (Pi's `--tools`); `excludeTools` is a denylist (Pi's `--exclude-tools`); `visibleBrowser: true` gives the child a visible Chrome on the persistent profile (it sets `OCTOCODE_BROWSER_VISIBLE=1`, as `webLive` does). The bundled `researcher` and `reviewer` set `excludeTools: file,browser,askUser`, `implementer` excludes `browser`, and `webHeadless` / `webLive` exclude `file` and set `mcp: false`. `mcp: false` starts the child without Pi's built-in MCP and without the `octocode` server (it sets `OCTOCODE_MCP=0`), so a profile that never researches code pays for no MCP tool schemas.

Subagents start with `--no-session` and this extension only, and cannot spawn further subagents. They do not run the user's command hooks unless their profile sets `hooks: true` (a hook meant for the main session would otherwise fire once per subagent). Each child receives a fresh run identity and profile-specific browser visibility; user settings such as MCP opt-out remain inherited. Their model usage is added to the parent session's totals, and a subagent whose model call fails is reported as a failed tool call.

## MCP

Octocode runs on Pi's built-in MCP (Pi 0.99.2 or later). On `session_start` it registers one server, `octocode` (the bundled `octocode-mcp` 19.1.0, or `npx -y octocode-mcp@19.1.0` when it is not installed next to the extension), with `pi.registerMcpServer`, `cwd` and `WORKSPACE_ROOT` set to the session's folder and `ALLOWED_PATHS` to that folder and the Octocode home. Its tools are named `mcp__octocode__<tool>`; GitHub tokens (`GITHUB_TOKEN`, …) reach it through Pi's environment. `OCTOCODE_MCP=0` skips the registration. Registration is refreshed on each session start, including a retry after registration failed.

Exposure: the local tools (`localSearch`, `localGetFileContent`, `localAnalyzeGraph`) and `lspGetSemantics` are `direct`, declared to the model from the first prompt. The GitHub tools (`gh*`) and `npmSearch` are `deferred` (Pi's `toolExposure`): Pi activates `tool_search`, lists the server in the `mcp_servers` prompt section, and declares those tools once `tool_search` loads them. This keeps most of the schema text out of requests that never touch GitHub or npm. `OCTOCODE_MCP_DIRECT=1` declares all nine directly.

The prompt's research guidance is chosen from the registration (and, with `OCTOCODE_MCP=0`, from a user-configured `octocode` server's tools once they appear), not from the request's tool snapshot: Pi takes that snapshot before its MCP host connects servers on the first prompt, so it would miss Octocode on the first turn and in single-prompt subagents. Once on, the guidance stays on for the session so the cached prompt does not change.

Octocode tool results are sanitized before the model reads them or the terminal draws them: terminal control sequences, bidi overrides and invisible characters (Unicode tags U+E0000–E007F among them) are stripped from text parts and structured content. This applies to any server named `octocode`.

A server named `octocode` in Pi's `mcp.json` takes precedence, which is how to override or disable it.

Every other server is configured with Pi's MCP: `~/.pi/agent/mcp.json`, `.pi/mcp.json`, `pi mcp add`, `/mcp` (status, reconnect, sign-in) and Pi's OAuth. See [Pi's MCP docs](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/mcp.md). `pi --no-extensions` also turns off Pi's built-in MCP; add `-e builtin:mcp -e builtin:tool-search` to keep it and the tool search that loads the deferred GitHub and npm tools (for example `pi --no-extensions -e packages/octocode-pi-extension/dist/index.js -e builtin:mcp -e builtin:tool-search`).

## Retention

A sweep runs at most every 6 hours across all sessions, off the session start path; it never touches the current session or Pi's own session files.

| What | Removed when |
|---|---|
| A session's folder and extras row, when Pi no longer lists the session (deleted, ephemeral, a subagent's) | 7 days after the folder was last written, unless its process is running |
| A `_pid-<pid>` folder (output written before a session id was known) | 1 day after that process died |
| Spilled `output/` and `bash/` logs inside a kept session | `OCTOCODE_CLEANUP_DAYS` days after they were written (default 30; `0` turns the whole sweep off) |
| Checkpoints | With their session's folder (and the 256-entry / 128 MiB cap per session) |
| Backlog items, notes and memories | Never automatically: they stay until deleted (items on `/backlog`; memories on `/memory` or with the `memory` tool's `delete`) |

`/sessions` → Forget Octocode data removes one session's folder and extras row at once.
