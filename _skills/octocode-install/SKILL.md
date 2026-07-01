---
name: octocode-install
description: Interactive step-by-step installer for Octocode tools on macOS and Windows. Use when the user asks to "install octocode", "set up octocode", "configure octocode mcp", "get started with octocode", "install octocode", "octocode setup", or needs help with GitHub auth, IDE MCP config, or skills installation.
---

# Octocode Install — Interactive Setup

`DETECT` → `CHECK INSTALLED` → `NODE` → `AUTH` → `INSTALL MCP` → `SKILLS` → `VERIFY`

**Agent rule**: Detect what you can from context. Ask only what you can't determine. One step at a time — wait for the user's answer before proceeding.

---

## Step 0 — Detect Platform & IDE

Check the conversation and environment context first:
- **Platform**: Is the OS already known? (macOS/Linux vs Windows)
- **IDE**: Is the user in Cursor, Claude Code, Claude Desktop, Windsurf, VS Code, Zed, Opencode, Trae, Kiro, Codex, Gemini CLI, Goose, Antigravity?

If either is **unknown**, ask:

> "What platform are you on, and which IDE/client are you setting up?"
> - macOS / Linux
> - Windows
>
> IDE: Cursor · Claude Code · Claude Desktop · Windsurf · Trae · Kiro · Antigravity · VS Code (Cline/Roo/Continue) · Zed · Opencode · Codex · Gemini CLI · Goose

Carry both answers through all remaining steps.

---

## Step 1 — Check if Already Installed

Read the IDE's MCP config file (see config paths table below) and check whether an `"octocode"` or `"octocode-mcp"` server entry already exists.

| IDE | Config path (macOS) | Config path (Linux) | Config path (Windows) |
|-----|---------------------|---------------------|-----------------------|
| Cursor | `~/.cursor/mcp.json` | `~/.cursor/mcp.json` | `%APPDATA%\Cursor\mcp.json` |
| Claude Code | `~/.claude.json` | `~/.claude.json` | `%USERPROFILE%\.claude.json` |
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` | `~/.config/claude/claude_desktop_config.json` | `%APPDATA%\Claude\claude_desktop_config.json` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `~/.codeium/windsurf/mcp_config.json` | `%USERPROFILE%\.codeium\windsurf\mcp_config.json` |
| Trae | `~/Library/Application Support/Trae/mcp.json` | `~/.config/Trae/mcp.json` | `%APPDATA%\Trae\mcp.json` |
| Kiro | `~/.kiro/mcp.json` | `~/.kiro/mcp.json` | `%APPDATA%\Kiro\mcp.json` |
| Antigravity | `~/.gemini/antigravity/mcp_config.json` | `~/.gemini/antigravity/mcp_config.json` | `~/.gemini/antigravity/mcp_config.json` |
| VS Code (Cline) | `~/Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json` | `~/.config/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json` | `%APPDATA%\Code\User\globalStorage\saoudrizwan.claude-dev\settings\cline_mcp_settings.json` |
| VS Code (Roo) | `~/Library/Application Support/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json` | `~/.config/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json` | `%APPDATA%\Code\User\globalStorage\rooveterinaryinc.roo-cline\settings\mcp_settings.json` |
| VS Code (Continue) | `~/.continue/config.json` | `~/.continue/config.json` | `~/.continue/config.json` |
| Zed | `~/.config/zed/settings.json` | `~/.config/zed/settings.json` | `%APPDATA%\Zed\settings.json` |
| Opencode | `~/Library/Application Support/opencode/config.json` | `~/.config/opencode/config.json` | `%APPDATA%\opencode\config.json` |
| Codex | `~/.codex/config.toml` | `~/.codex/config.toml` | `~/.codex/config.toml` |
| Gemini CLI | `~/.gemini/settings.json` | `~/.gemini/settings.json` | `~/.gemini/settings.json` |
| Goose | `~/Library/Application Support/goose/config.yaml` | `~/.config/goose/config.yaml` | `%APPDATA%\goose\config.yaml` |

Also run:
```bash
npx octocode status
```

**If octocode-mcp is already configured in the IDE AND authenticated:**

> Tell the user: "Octocode is already installed and authenticated in your IDE. You're all set!"
>
> Offer: "Would you like to **update** (`npx octocode install --ide <key> --force`), **install skills**, or **change config**?"

**STOP here** — do not continue with Steps 2–6 unless the user asks to update, reinstall, or add something.

---

## Step 2 — Node.js

```bash
node --version
```

- **v18+** → continue
- **Missing or old** → tell the user to install it, then wait for confirmation:
  - macOS: `brew install node` or [nodejs.org](https://nodejs.org)
  - Windows: `winget install OpenJS.NodeJS` or [nodejs.org](https://nodejs.org)

> Ask: "Does `node --version` show v18 or higher now?"

---

## Step 3 — GitHub Authentication

**Ask the user:**

> "How would you like to authenticate with GitHub?"
> 1. **`npx octocode login`** — Octocode OAuth (opens browser, stores token automatically)
> 2. **`gh auth login`** — GitHub CLI (if `gh` is already installed)
> 3. **GitHub PAT (token)** — paste a token manually; always works, required on Windows if browser auth fails

---

### Option 1 — Octocode OAuth (recommended)

```bash
npx octocode login
```

- Opens browser → approve the device code → done
- Token stored encrypted at `~/.octocode/credentials.json`
- Automatically used by `octocode-mcp` — no env var needed

> **Windows**: If the browser doesn't open or the command hangs, switch to Option 3 (PAT).

---

### Option 2 — GitHub CLI

Requires `gh` already installed ([cli.github.com](https://cli.github.com)):

```bash
gh auth login
```

- Follow the interactive prompts
- `octocode-mcp` reads the token via `gh auth token` automatically

---

### Option 3 — GitHub PAT (always works, required on Windows if OAuth fails)

1. Create token at [github.com/settings/tokens/new](https://github.com/settings/tokens/new)
   - Scopes: `repo` + `read:org`
2. Copy the token — you'll paste it as `GITHUB_TOKEN` in the MCP config (Step 4)

---

**After auth, verify:**
```bash
npx octocode status
```

Expected: `✓ Authenticated as <username>`

If not authenticated — repeat the chosen option or switch to Option 3.

---

## Step 4 — Install MCP for IDE

**If IDE is unknown, ask:**
> "Which IDE/client do you want to install octocode-mcp into?"

Run:
```bash
npx octocode install --ide <key>
```

| IDE | `<key>` | Aliases |
|-----|---------|---------|
| Cursor | `cursor` | |
| Claude Code | `claude-code` | `claudecode` |
| Claude Desktop | `claude-desktop` | `claude`, `claudedesktop` |
| Windsurf | `windsurf` | |
| Trae | `trae` | |
| Kiro | `kiro` | |
| Antigravity | `antigravity` | |
| VS Code (Cline) | `vscode-cline` | `cline` |
| VS Code (Roo) | `vscode-roo` | `roo`, `roo-cline` |
| VS Code (Continue) | `vscode-continue` | `continue` |
| Zed | `zed` | |
| Opencode | `opencode` | |
| Codex | `codex` | |
| Gemini CLI | `gemini-cli` | `gemini`, `geminicli` |
| Goose | `goose` | |

The CLI writes the config file automatically. Then confirm the result:

### What the config looks like

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["@octocodeai/mcp@latest"]
    }
  }
}
```

### Ask — enable local tools

> "Do you want to enable **local codebase tools** (search files, LSP, browse dirs)? **Recommended — Yes.**"
> → Yes: add `"ENABLE_LOCAL": "true"` to `"env"`

> "Did you use a PAT (Option 3) for auth, or is `npx octocode status` not showing authenticated?"
> → Yes: add `"GITHUB_TOKEN": "ghp_xxx"` to `"env"`

### Full config with all options:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["@octocodeai/mcp@latest"],
      "env": {
        "GITHUB_TOKEN": "ghp_xxxx",
        "ENABLE_LOCAL": "true"
      }
    }
  }
}
```

> **Token resolution order** (octocode-mcp picks the first found):
> `OCTOCODE_TOKEN` → `GH_TOKEN` → `GITHUB_TOKEN` → `~/.octocode/credentials.json` → `gh auth token`
> If you used OAuth or `gh auth login`, you can omit `GITHUB_TOKEN`.

**Restart the IDE after saving the config.**

---

## Step 5 — Install Octocode Skills

**Ask the user:**

> "Would you like to install all Octocode skills? These add research, planning, code review, coordination, skill work, stats, and more to your AI assistant."

- **No** → skip to Step 6
- **Yes** → ask which targets:

> "Which AI clients should skills be installed into?"
> - **Current IDE only** (detected in Step 0)
> - **All supported platforms** (`all`)

| Target | Skills directory |
|--------|----------------|
| `common` | `~/.agents/skills/` |
| `claude` | `~/.claude/skills/` and `~/.claude-desktop/skills/` |
| `cursor` | `~/.cursor/skills/` |
| `codex` | `~/.agents/skills/` |
| `opencode` | `~/.config/opencode/skills/` |
| `pi` | `~/.pi/agent/skills/` |
| `copilot` | `~/.copilot/skills/` |
| `gemini` | `~/.gemini/skills/` |

Then run:
```bash
npx octocode skill --install-all --platform <selected-platforms> --force
```

This installs the bundled Octocode skills:

| Skill | What it does |
|-------|-------------|
| `octocode` | Quick Octocode MCP or CLI code lookup |
| `octocode-awareness` | Memory, file locks, handoffs, and verify-before-done |
| `octocode-brainstorming` | Idea validation and prior-art decision briefs |
| `octocode-research` | Code exploration, multi-source research, implementation, refactoring, PR review, and evidence loops |
| `octocode-rfc-generator` | Technical design documents, RFCs, and research-backed plans |
| `octocode-roast` | Brutally honest code review |
| `octocode-skills` | Search, evaluate, install, create, lint, and update Agent Skills |
| `octocode-stats` | Local Octocode usage, savings, cache, error, and rate-limit dashboard |

After install, verify:
```bash
npx octocode skill --list
```

Expected: every official skill appears in the named-skill catalog.

---

## Step 6 — Verify

```bash
npx octocode status       # auth check
npx octocode skill --list # skills catalog check
```

Then open the IDE and test:
> "Use octocode-research to find the main entry point of this project"

Tools responding = setup complete.

---

## Using Skills

| Goal | Say... |
|------|--------|
| Find/explore code | "Find where X is defined", "Who calls Y?" |
| Understand/implement | "How does X work?", "Implement this" |
| Plan | "Plan how to add feature X" |
| Review PR | "Review PR #123" |
| Document | "Document this project" |
| Roast | "Roast my code" |

- Local tools require `ENABLE_LOCAL=true` in MCP config
- Skill not triggering? Name it explicitly: "use octocode-research to..."
- More skills: `npx octocode` → Manage Skills → Browse Marketplace

---

## Troubleshooting

| Problem | Fix |
|---------|-----|
| `npx: command not found` | Install Node.js v18+ |
| OAuth browser doesn't open (Windows) | Use PAT — set `"GITHUB_TOKEN"` in MCP config `"env"` |
| `npx octocode status` shows not authenticated | Add `"GITHUB_TOKEN": "ghp_xxx"` to MCP `"env"` |
| Local tools return nothing | Add `"ENABLE_LOCAL": "true"` to MCP `"env"`, restart IDE |
| Skills not loading | Verify `~/<client>/skills/<skill>/SKILL.md` has `name` + `description` |
| Already installed, want to update | Add `--force`: `npx octocode install --ide cursor --force` |

Docs: [Auth](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md) · [CLI Reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md)
