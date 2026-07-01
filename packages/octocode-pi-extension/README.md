# @octocodeai/pi-extension

<div align="center">
  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-pi-extension/assets/logo.png" width="640px" alt="Octocode + Pi">
</div>

> An evidence harness for Pi that makes your coding agent investigate, reason, and verify like a senior engineer on every task.

## The problem

[Pi](https://github.com/earendil-works/pi) is a lean, open coding-agent CLI: fast, minimal, and unopinionated by design. That leanness is the point, but a blank-slate agent guesses. It matches text instead of grasping meaning, treats a search hit as proof, forgets what it decided when context runs out, and rebuilds the same research loop every session.

A coding agent is only as good as the evidence it works from.

## What this does

This package is the missing engineering discipline for Pi. One `pi install` turns a blank-slate agent into an evidence-first one: research before assumptions, proof before edits, memory that survives the session. It is live on every turn with no config, no `npx`, and nothing to wire up.

It works because three layers reinforce each other:

```
┌──────────────────────────────────────────────────────────┐
│                         Pi Agent                          │
│                                                           │
│   System prompt   +   Octocode CLI    +       Skills      │
│   (how to think)      (the research      (proven research │
│                        tool it uses)        workflows)    │
└──────────────────────────────────────────────────────────┘
```

The **system prompt** sets the operating model (how to reason and when to prove). The **Octocode CLI** is the one tool the agent reaches for to understand code. The **skills** are ready-made workflows for the harder jobs. They ship together, so the prompt, the tool, and the workflows never drift apart. Pi stays lean; this extension is what makes it senior-grade.

## Quick start

**1. Install** (global by default, or add `-l` for project-local):

```bash
pi install npm:@octocodeai/pi-extension
```

> Pi packages run with full system access. Review the package before installing in a sensitive environment.

**2. Authenticate** to GitHub once. Octocode stores the token where both you and the agent's bundled CLI read it:

```bash
npx octocode auth login    # store a GitHub token (interactive)
npx octocode auth status   # confirm you are authenticated
```

> Agents can skip login by passing `GITHUB_TOKEN`, `OCTOCODE_TOKEN`, or `GH_TOKEN` via env. See the [authentication docs](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md) for all options.

**3. Verify** the CLI, skills, awareness state, and prompt all loaded:

```bash
/octocode-status
```

That is the full setup. The next two steps are optional.

**Pin the system prompt to disk** so it is visible in your project:

```bash
/octocode-setup            # writes .pi/APPEND_SYSTEM.md
/octocode-setup --global   # writes ~/.pi/agent/APPEND_SYSTEM.md
```

**Use the bundled CLI binary directly** instead of `npx octocode`: copy the path from `/octocode-status` (shown as `octocode CLI: bundled … → <path>`) and run `node <path> auth login`.

## The three layers

### 1. System prompt: how the agent thinks

A short operating model, built for the failure modes of coding agents, injected on every turn:

```
orient → hypothesize → search/read → prove → act → verify
```

Most agent failures happen between *"I found something"* and *"I changed something."* This loop closes that gap. Its core rules target the exact places coding agents go wrong:

- **Search results are leads, not proof.** A hit is a hypothesis; an exact file read or a passing test is evidence.
- **Verify ground truth first.** Check `git status`, manifests, and environment before assuming state.
- **Do not write code you do not need.** Reuse, stdlib, and existing deps come before new code.
- **Fix root causes.** Find every caller before changing a shared function.
- **Verify before claiming done.** Leave one runnable self-check for every real change.

Read the full operating model in [`APPEND_SYSTEM.md`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/PI/APPEND_SYSTEM.md).

### 2. Octocode CLI: the research tool

[Octocode](https://octocode.ai) is one research engine for everything the agent needs to understand (local code, GitHub, and npm) instead of juggling `grep`, `find`, `cat`, `gh`, and `npm`. It combines full-text search, LSP semantics, and AST matching, and reads token-lean (symbols, then compact, then exact) so the agent spends context only where it pays.

This package bundles it as a CLI, which is the right fit for a lean CLI-first platform: the agent calls it as a shell command, with no MCP server to run and no protocol overhead. Because it ships inside the package there is no separate download and no version drift between the CLI, the skills, and the prompt. It is ready the moment install finishes, and the agent is told exactly where to find it.

See the [Octocode CLI docs](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLI.md) for the full command surface. If you prefer structured tool calls over shell, the same engine is available as MCP tools ([below](#optional-octocode-mcp-inside-pi)).

### 3. Skills: reusable research workflows

Tested workflows the agent activates on demand instead of improvising. They load automatically on install. Pi picks the right one from context, or you can invoke one directly with `/skill:<name>`.

| Skill | What it does |
|---|---|
| [`octocode-research`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-research) | Evidence-first investigation: code research, implementation, PR/diff review, refactor, dead-code, architecture mapping, binary/artifact inspection |
| [`octocode-awareness`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-awareness) | Durable memory and file locks: claim files before editing, record decisions, hand off across sessions |
| [`octocode-brainstorming`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-brainstorming) | Evidence-grounded idea and prior-art exploration before building (needs a web search key: `SERPER_API_KEY` or `TAVILY_API_KEY`) |
| [`octocode-rfc-generator`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-rfc-generator) | Structured proposals for risky or cross-cutting work |
| [`octocode-roast`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-roast) | Adversarial code review with severity-ranked findings |
| [`octocode-skills`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-skills) | Find, install, rate, and create skills |

## Slash commands

| Command | Purpose |
|---|---|
| `/octocode-status` | Show bundled CLI, prompt, and skills. Verify everything loaded |
| `/octocode-setup [--global]` | Write the system prompt to disk (project or global) |
| `/octocode-mcp-install [args]` | Run the bundled `octocode install` for MCP-native hosts |
| `/octocode-skills-update` | Update the package and reload skills |

## Optional: Octocode MCP inside Pi

The bundled CLI already covers the full research surface. Add MCP only if you want Pi to call Octocode through structured tool calls instead of shell commands.

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

## Links

- [Octocode](https://octocode.ai) · [Octocode MCP](https://github.com/bgauryy/octocode-mcp)
- [Pi packages](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/packages.md) · [extensions](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md) · [skills](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/skills.md)
- [Pi MCP adapter](https://github.com/nicobailon/pi-mcp-adapter)
</content>
</invoke>
