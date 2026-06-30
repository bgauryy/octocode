# @octocodeai/pi-extension

<div align="center">
  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-pi-extension/assets/logo.png" width="640px" alt="Octocode + Pi">
</div>

> A complete research-quality evidence harness for Pi — so your agent investigates, reasons, and verifies like a senior engineer across every development and research task.

---

## Why this exists

A coding agent is only as good as the evidence it works from. Left on its own, it guesses: it matches text instead of grasping meaning, treats a search hit as proof, forgets what it decided when context runs out, and rebuilds the same research loop every session.

This package closes that gap. It turns a blank-slate Pi agent into an evidence-first one — research before assumptions, proof before edits, memory that survives the session. One `pi install`, and it's live on every turn: no config, no `npx`, nothing to wire up.

It does this by bundling three things that reinforce each other.

---

## Quick start

**1. Install** (globally, available in every project — or add `-l` for project-local only):

```bash
pi install npm:@octocodeai/pi-extension
```

> Pi packages run with full system access. Review the package before installing in a sensitive environment.

**2. Verify** everything loaded — shows the bundled CLI, skills, awareness state, and prompt location:

```bash
/octocode-status
```

**3. Authenticate** to GitHub once — Octocode stores the token where both you and the agent's bundled CLI read it:

```bash
npx octocode auth login     # store a GitHub token (interactive)
npx octocode auth status     # confirm you're authenticated
```

> `npx octocode` works anywhere. To use the bundled binary directly instead, copy the path from `/octocode-status` (shown as `octocode CLI: bundled … → <path>`) and run `node <path> auth login`. Agents can skip login entirely by passing `GITHUB_TOKEN` / `OCTOCODE_TOKEN` / `GH_TOKEN` via env. See the [authentication docs](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md) for all options.

**4. (Optional) Pin the system prompt to disk** so it's visible in your project:

```bash
/octocode-setup            # writes .pi/APPEND_SYSTEM.md
/octocode-setup --global   # writes ~/.pi/agent/APPEND_SYSTEM.md
```

---

## What's in the bundle

```
┌──────────────────────────────────────────────────────────┐
│                         Pi Agent                          │
│                                                           │
│   System prompt   +   Octocode CLI    +       Skills      │
│   (how to think)      (the research      (how to research:│
│                        tool it uses)     proven workflows)│
└──────────────────────────────────────────────────────────┘
```

### 1. The system prompt — how the agent thinks

A short operating model injected on every turn:

```
orient → hypothesize → search/read → prove → act → verify
```

Most agent failures happen between *"I found something"* and *"I changed something."* This loop closes that gap. The core rules:

- **Search results are leads, not proof** — a hit is a hypothesis; an exact file read or a passing test is evidence.
- **Verify ground truth first** — check `git status`, manifests, and environment before assuming state.
- **Don't write code you don't need** — reuse, stdlib, and existing deps come before new code.
- **Fix root causes** — find every caller before changing a shared function.
- **Verify before claiming done** — leave one runnable self-check for every real change.

### 2. The Octocode CLI — the research tool, bundled

One research tool for everything the agent needs to understand — local code, GitHub, and npm — instead of juggling `grep` / `find` / `cat` / `gh` / `npm`. It's bundled with the package, so there's **no separate download and no version drift** between the CLI, the skills, and the prompt: ready the moment install finishes, with the agent told exactly where to find it.

See the [Octocode CLI docs](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md) for the full command surface.

### 3. The skills — reusable research workflows

Tested workflows the agent activates on demand instead of improvising. They load automatically when the package is installed.

| Skill | What it does |
|---|---|
| [`octocode-research`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-research) | Evidence-first investigation with preset workflows — code research, implementation, PR/diff review, refactor, dead-code, architecture mapping, binary/artifact inspection |
| [`octocode-awareness`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-awareness) | Durable memory + file locks — claim files before editing, record decisions, hand off across sessions |
| [`octocode-brainstorming`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-brainstorming) | Evidence-grounded idea and prior-art exploration before building — needs a web search key (`SERPER_API_KEY` or `TAVILY_API_KEY`) |
| [`octocode-rfc-generator`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-rfc-generator) | Structured proposals for risky or cross-cutting work |
| [`octocode-roast`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-roast) | Adversarial code review with severity-ranked findings |
| [`octocode-skills`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-skills) | Find, install, rate, and create skills |

Pi picks the right skill from context, or you can invoke one directly with `/skill:<name>`. Each skill links to its own README above.

---

## Slash commands

| Command | Purpose |
|---|---|
| `/octocode-status` | Show bundled CLI, prompt, and skills — verify everything loaded |
| `/octocode-setup [--global]` | Write the system prompt to disk (project or global) |
| `/octocode-mcp-install [args]` | Run the bundled `octocode install` for MCP-native hosts |
| `/octocode-skills-update` | Update the package and reload skills |

---

## Optional: Octocode MCP inside Pi

The bundled CLI already covers the full research surface. Add MCP only if you specifically want Pi to call Octocode through structured tool calls instead of shell commands.

```bash
pi install npm:pi-mcp-adapter
```

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "@octocodeai/mcp@latest"]
    }
  }
}
```

---

## Links

- [Octocode](https://octocode.ai) · [Octocode MCP](https://github.com/bgauryy/octocode-mcp)
- [The system prompt (`APPEND_SYSTEM.md`)](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/PI/APPEND_SYSTEM.md) — the full operating model
- [Pi packages](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/packages.md) · [extensions](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md) · [skills](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/skills.md)
- [Pi MCP adapter](https://github.com/nicobailon/pi-mcp-adapter)
