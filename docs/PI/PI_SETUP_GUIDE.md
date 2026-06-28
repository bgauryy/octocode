# Using octocode-mcp with Pi

> **Pi documentation:** https://pi.dev/docs/latest

Pi is a CLI coding agent whose philosophy is *"CLI tools with READMEs (Skills) over MCP."* Pairing it with Octocode gives a lean, evidence-driven loop — **Pi edits; Octocode researches.**

Use the Octocode CLI (`npx octocode`) for all research. Install skills to extend Pi's capabilities with focused, evidence-driven workflows.

> **Path convention used throughout this guide**
> - `./` → Pi's global agent directory (`~/.pi/agent/`)
> - `.pi/` (no `~`) → per-project directory inside your current repo

---

## 1. Install skills

[Agent Skills](https://agentskills.io) (`SKILL.md` folders) are Pi's preferred extension model. Pi loads them from `~/.pi/agent/skills/` globally and from `.pi/skills/` inside trusted projects. Skills are activated automatically by task context or forced with `/skill:name`.

Browse all Octocode skills: **[skills.sh/bgauryy/octocode-mcp](https://www.skills.sh/bgauryy/octocode-mcp)**

| Skill | Purpose |
| --- | --- |
| ⭐ `octocode-engineer` | Codebase understanding, implementation, bug investigation, refactors, PR review, and RFC validation — with AST + LSP evidence |
| `octocode-research` | Deep code exploration: trace flow, find usages, understand a codebase |
| `octocode-brainstorming` | Validate ideas against GitHub, npm, and web evidence; produces a decision-ready brief. Add a [Tavily API key](https://tavily.com) (`TAVILY_API_KEY`) for richer web search results |
| `octocode-rfc-generator` | Evidence-backed RFCs and design docs before starting implementations |
| `octocode-loop` | Repeat Act -> Observe -> Learn loops until evidence converges |
| `octocode-awareness` | Shared memory, file locks, handoffs, and verify-before-done discipline |
| `octocode-roast` | Blunt but actionable code-quality critique |
| `octocode-skills` | Search, evaluate, install, create, and update Agent Skills |
| `octocode-stats` | Local Octocode usage and savings dashboard |
| `octocode` | Quick Octocode transport/router skill |

Install the recommended starting skill into Pi's global skills directory:

```bash
npx octocode skill --name octocode-engineer --platform pi
```

Install every current Octocode skill into Pi:

```bash
for skill in octocode octocode-awareness octocode-brainstorming octocode-engineer octocode-loop octocode-research octocode-rfc-generator octocode-roast octocode-skills octocode-stats; do
  npx octocode skill --name "$skill" --platform pi --mode copy --update
done
```

Use `npx octocode skill --list` to see the live named-skill catalog, and `npx octocode skill --help` for the active flags. Pi discovers skills automatically on next start; force one with `/skill:octocode-engineer`.

**Fallback — if you need a project-local Pi skill instead of the global `--platform pi` target:**

```bash
npx -y degit bgauryy/octocode/skills/<skill-name> .pi/skills/<skill-name>
```

---

## 2. Authenticate GitHub

Octocode GitHub tools need a token for private repositories and higher API rate limits. Any one method is enough:

**`npx octocode auth` (recommended):**

```bash
npx octocode auth login
npx octocode status   # confirm the active token source
```

**GitHub CLI (also supported):**

```bash
gh auth login
```

Octocode reads the `gh` token automatically — no further config needed.

**Personal Access Token (also supported):**

Set `OCTOCODE_TOKEN`, `GH_TOKEN`, or `GITHUB_TOKEN` in your shell. Required scopes: `repo`, `read:user`, `read:org`.

> Never commit tokens to version control. Use environment variables or a secret manager.

---

## 3. Tune Pi's behavior

Pi extends its system prompt from `APPEND_SYSTEM.md`. Use it to tell the agent which tools to prefer, any project conventions, and hard constraints. A ready-to-use starter lives at [`docs/PI/APPEND_SYSTEM.md`](https://github.com/bgauryy/octocode/blob/main/docs/PI/APPEND_SYSTEM.md).

| File | Scope | When loaded |
| --- | --- | --- |
| `.pi/APPEND_SYSTEM.md` | Project | When the project is trusted |
| `./APPEND_SYSTEM.md` | Global (`~/.pi/agent/`) | Always |

A trusted project's `.pi/APPEND_SYSTEM.md` **shadows** the global file — they do not merge. Keep cross-project rules global; put repo-specific rules in the project file.

`SYSTEM.md` (same locations) **replaces** the default prompt entirely — only use it when you need full control.

```bash
$EDITOR ~/.pi/agent/APPEND_SYSTEM.md
```

Keep it concise. A bloated file degrades adherence to all rules. Lead with hard constraints, name the exact tools to use, and push anything a linter can enforce into tooling rather than prose. Restart Pi to pick up changes.

---

## 4. Add custom models

Point Pi at additional providers via `~/.pi/agent/models.json`. The file reloads every time you open `/model` — no restart needed.

Each provider entry needs: `baseUrl`, `api` (one of `openai-completions`, `openai-responses`, `anthropic-messages`, `google-generative-ai`), `apiKey`, and a `models` array. `apiKey` accepts a literal string, `$ENV_VAR`, or `!shell-command` (e.g. `!op read 'op://vault/item/field'`) — don't commit raw secrets.

> Naming a provider after a built-in (`anthropic`, `openai`) and providing `models` **replaces** that provider's model list entirely. Use `modelOverrides` to extend the built-ins instead.

```json
{
  "providers": {
    "anthropic": {
      "baseUrl": "https://your-gateway/anthropic",
      "apiKey": "$ANTHROPIC_API_KEY",
      "api": "anthropic-messages",
      "models": [
        {
          "id": "claude-sonnet-4-6",
          "reasoning": true,
          "input": ["text", "image"],
          "contextWindow": 200000,
          "maxTokens": 32000
        }
      ]
    },
    "openai": {
      "baseUrl": "https://your-gateway/openai/v1",
      "apiKey": "$OPENAI_API_KEY",
      "api": "openai-completions",
      "models": [
        {
          "id": "gpt-5.5",
          "input": ["text", "image"],
          "contextWindow": 200000
        }
      ]
    }
  }
}
```

Select a model with `/model` inside Pi, or pass `--model <pattern>` at launch.

---

## References

- [Pi documentation](https://pi.dev/docs/latest)
- [Pi source](https://github.com/earendil-works/pi)
- [Octocode skills index](https://www.skills.sh/bgauryy/octocode-mcp)
- [APPEND_SYSTEM.md starter](https://github.com/bgauryy/octocode/blob/main/docs/PI/APPEND_SYSTEM.md)
- Octocode CLI tool reference: [GitHub tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/GITHUB_TOOLS.md) · [Local tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LOCAL_TOOLS.md) · [LSP tools](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LSP_TOOLS.md)
