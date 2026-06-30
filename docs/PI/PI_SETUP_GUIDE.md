# Using octocode-mcp with Pi

> **Pi documentation:** https://pi.dev/docs/latest
> **Octocode harness:** https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_HARNESS.md

Pi is a terminal coding agent that keeps the core harness small and pushes
workflow-specific behavior into skills, context files, shell tools, extensions,
and packages. That is exactly where Octocode fits: **Pi stays lean; Octocode
supplies research, memory, planning, review, and skill-maintenance workflows.**

Use this cookbook to install Pi, add the Octocode system-prompt addendum,
install the Octocode skills, and optionally configure Octocode MCP for other
MCP-native clients.

> **Path convention used throughout this guide**
> - `./` -> Pi's global agent directory (`~/.pi/agent/`)
> - `.pi/` -> per-project directory inside your current repo

---

## 1. Install Pi

The Pi and Octocode examples below assume a Node.js/npm environment that
provides `npm` and `npx`. Install Pi from npm, then start it from the project
directory you want it to work on:

```bash
npm install -g --ignore-scripts @earendil-works/pi-coding-agent
cd /path/to/project
pi
```

On Linux or macOS, Pi also publishes an installer:

```bash
curl -fsSL https://pi.dev/install.sh | sh
cd /path/to/project
pi
```

Inside Pi, authenticate a model provider with `/login`, or launch Pi with the
provider API key already in the environment:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
pi
```

Useful Pi startup commands:

```bash
pi                     # Start an interactive session
pi -c                  # Continue the most recent session
pi -r                  # Browse previous sessions
pi -p "Summarize this" # Print mode for one-off automation
```

Why Pi for the Octocode harness:

- Pi is shell-native, so `npx octocode ...` calls are easy for the agent to run,
  quote, rerun, and verify.
- Pi loads skills on demand, so the base prompt stays lean instead of carrying
  every workflow all the time.
- Pi context files let you add the Octocode harness without replacing Pi's
  default behavior.
- Pi's core deliberately stays small; workflows such as subagents, plan mode,
  permission gates, MCP, and sandboxing are optional extensions or environment
  choices.
- Pi remains the editing loop; Octocode becomes the evidence layer.

---

## 2. Install Octocode CLI And Auth

Start with the CLI because it bootstraps research, skill installs, auth, and MCP
setup:

```bash
npx octocode --help
npx octocode auth login
npx octocode status --json
```

If `npx` is not available, install Node.js/npm or use your package manager's
equivalent package-runner command. The important invariant is that Pi can run
the same Octocode command again for verification.

Octocode can also use the GitHub CLI token:

```bash
gh auth login
```

Or pass a token through the environment:

```bash
export GITHUB_TOKEN=...
# Also supported: GH_TOKEN or OCTOCODE_TOKEN
```

Never commit tokens. Use environment variables, Pi auth storage, `gh`, or a
secret manager.

---

## 3. Add The Octocode System Prompt To Pi

Pi loads context files and system-prompt addenda at startup. Use
`APPEND_SYSTEM.md` to add the Octocode harness while preserving Pi's default
prompt.

Install the global Octocode prompt:

```bash
mkdir -p ~/.pi/agent
curl -fsSL \
  https://raw.githubusercontent.com/bgauryy/octocode/main/docs/PI/APPEND_SYSTEM.md \
  -o ~/.pi/agent/APPEND_SYSTEM.md
```

Or install it only for one trusted project:

```bash
mkdir -p .pi
curl -fsSL \
  https://raw.githubusercontent.com/bgauryy/octocode/main/docs/PI/APPEND_SYSTEM.md \
  -o .pi/APPEND_SYSTEM.md
```

Then restart Pi or run `/reload`.

| File | Scope | Behavior |
| --- | --- | --- |
| `~/.pi/agent/APPEND_SYSTEM.md` | Global | Appended in every Pi session |
| `.pi/APPEND_SYSTEM.md` | Project | Appended when the project is trusted |
| `SYSTEM.md` in either location | Global/project | Replaces Pi's default system prompt |

Pi supports `APPEND_SYSTEM.md` in either global or project locations. Avoid
depending on both locations for the same rule unless you have checked the
current Pi startup header or source for the exact load order. Keep
cross-project Octocode rules global and repo-specific rules in project files.

Avoid `SYSTEM.md` unless you intentionally want to replace Pi's default prompt.
The Octocode harness is designed as an append file.

---

## 4. Install The Octocode Skills

Pi scans skill folders at startup and exposes skills as `/skill:name`. Install
all official Octocode skills into Pi:

```bash
npx octocode skill --install-all --platform pi --update
```

That refreshes the canonical source in `~/.octocode/skills` and links the skills
into `~/.pi/agent/skills` by default. Check the live catalog and installer flags
before automating:

```bash
npx octocode skill --list
npx octocode skill --help
```

To install only the core harness skills:

```bash
npx octocode skill --name octocode --platform pi --update
npx octocode skill --name octocode-research --platform pi --update
npx octocode skill --name octocode-awareness --platform pi --update
npx octocode skill --name octocode-brainstorming --platform pi --update
npx octocode skill --name octocode-rfc-generator --platform pi --update
npx octocode skill --name octocode-roast --platform pi --update
npx octocode skill --name octocode-skills --platform pi --update
```

After installation, restart Pi or run `/reload`. Let Pi auto-trigger skills from
task context, or force one explicitly:

```text
/skill:octocode-research
/skill:octocode-awareness
/skill:octocode-brainstorming
/skill:octocode-rfc-generator
/skill:octocode-roast
/skill:octocode-skills
```

Project-local fallback:

```bash
mkdir -p .pi/skills
npx -y degit bgauryy/octocode/skills/octocode-research .pi/skills/octocode-research
```

Use project-local skills when a repo needs a pinned copy; otherwise prefer the
global Pi install.

---

## 5. Skills Cookbook

Use the smallest skill that matches the task. `octocode-research` is the default
for technical work; the others specialize the moment.

| Skill | Use it for | Say this in Pi |
| --- | --- | --- |
| `octocode` | Quick Octocode transport and lookup across local files, GitHub, npm, PR/history, LSP, AST, and artifacts. | "Use Octocode to find where this API is implemented, then read the exact files." |
| `octocode-research` | Default research skill: local and external code research, implementation, debugging, code reviews, PR/commit history, binary/archive inspection, local AST search, and LSP semantics. | "Use octocode-research to investigate this bug, prove the root cause, patch it, and verify." |
| `octocode-awareness` | Agent awareness: memory, file locks, handoffs, notifications, verification records, and learning capture across long or concurrent work. | "Use octocode-awareness before editing, remember the decision, and verify before done." |
| `octocode-brainstorming` | Brainstorm and validate ideas from articles, code, GitHub, packages, and web evidence; use perspective debate before committing. | "Use octocode-brainstorming to test this product idea against prior art and code evidence." |
| `octocode-rfc-generator` | Pre-plan risky work: RFCs, migration plans, architecture proposals, implementation plans, and option comparisons. | "Use octocode-rfc-generator to write an RFC before we touch code." |
| `octocode-roast` | Hard code critique with cited findings and repair paths. | "Use octocode-roast on this module, then turn the highest-impact issues into fixes." |
| `octocode-skills` | Search best skill/code resources, compare skills, create/update skills, lint skill folders, and improve the harness. | "Use octocode-skills to find the best prior art, update this skill, and run the linter." |

Common harness routes:

| User goal | Pi + Octocode route |
| --- | --- |
| Understand unfamiliar code | `octocode` -> `octocode-research` |
| Fix a bug | `octocode-research` -> `octocode-awareness` verification |
| Review a PR or local diff | `octocode-research` review mode, or `octocode-roast` when the user wants blunt critique |
| Validate an idea | `octocode-brainstorming` -> `octocode-rfc-generator` if it survives |
| Plan a migration | `octocode-research` -> `octocode-rfc-generator` |
| Edit a skill | `octocode-skills` -> `octocode-awareness` handoff |
| Long or concurrent task | Add `octocode-awareness` at the start and before final response |

Good Pi prompts:

```text
Use the Octocode harness. First orient with octocode-research, then edit only
after exact evidence, and verify with the smallest meaningful command.
```

```text
Use octocode-awareness for this task. Claim files before editing, keep a handoff
note if context grows, and record any reusable lesson.
```

```text
Use octocode-brainstorming. Check articles, GitHub, packages, and this repo,
then run the Critical Architect / Visionary Entrepreneur / Product review and
tell me whether this deserves an RFC.
```

```text
Use octocode-skills to improve this skill. Search prior art, inspect the local
skill, update it, and run the skill lint/eval checks.
```

---

## 6. Optional: Add Pi Extensions And Safety Boundaries

Pi core intentionally does not bake in MCP, subagents, permission popups, plan
mode, to-dos, background bash, or a default sandbox. Add those only when the
workflow needs them:

| Need | Pi route | Why it matters for Octocode |
| --- | --- | --- |
| Subagents | Use a Pi subagent extension/package or a separate `pi -p` worker. | Offload external research, broad source triage, summaries, and fresh-context review. |
| Plan mode | Use a plan-mode extension or write plans to files. | Keeps risky work read-only until the plan is accepted. |
| Permission/path gates | Use extensions such as permission gates or protected paths. | Adds local policy around dangerous commands and sensitive files. |
| Sandbox | Use Gondolin, Docker, OpenShell, or another wrapper. | Pi otherwise runs with the permissions of the launching user/process. |
| Background work | Use tmux, a separate terminal, or a purpose-built extension. | Pi does not provide background bash as a core feature. |

These are opportunities, not prerequisites. The recommended default remains:
Pi + Octocode skills + Octocode CLI. Add extensions when the project needs more
workflow structure or stronger execution boundaries.

---

## 7. Optional: Add Octocode MCP

Pi's recommended Octocode path is skills plus CLI. Pi core does not include
built-in MCP; Octocode MCP is optional and most useful when you also use an
MCP-native editor or agent host.

Install into a supported MCP client:

```bash
npx octocode install --ide cursor --check
npx octocode install --ide cursor
```

Other supported client IDs are listed by:

```bash
npx octocode install --help
```

Manual MCP configuration:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "type": "stdio",
      "args": ["-y", "@octocodeai/mcp@latest"],
      "env": {
        "ENABLE_LOCAL": "true",
        "ENABLE_CLONE": "true"
      }
    }
  }
}
```

Use MCP tools directly when the host exposes them. In Pi, keep using the CLI:
`npx octocode ...`.

---

## 8. Add Custom Models

Point Pi at additional providers via `~/.pi/agent/models.json`. The file reloads
every time you open `/model`; no restart needed.

Each provider entry needs `baseUrl`, `api`, `apiKey`, and a `models` array.
`api` is one of `openai-completions`, `openai-responses`,
`anthropic-messages`, or `google-generative-ai`. `apiKey` accepts a literal
string, `$ENV_VAR`, or `!shell-command` such as
`!op read 'op://vault/item/field'`. Replace the placeholder model IDs below
with IDs supported by your gateway. Do not commit raw secrets.

> Naming a provider after a built-in (`anthropic`, `openai`) and providing
> `models` replaces that provider's model list entirely. Use `modelOverrides` to
> extend built-ins instead.

```json
{
  "providers": {
    "anthropic": {
      "baseUrl": "https://your-gateway/anthropic",
      "apiKey": "$ANTHROPIC_API_KEY",
      "api": "anthropic-messages",
      "models": [
        {
          "id": "your-anthropic-model-id",
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
          "id": "your-openai-compatible-model-id",
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
- [Pi quickstart](https://pi.dev/docs/latest/quickstart)
- [Pi usage and context files](https://pi.dev/docs/latest/usage)
- [Pi skills](https://pi.dev/docs/latest/skills)
- [Pi extensions](https://pi.dev/docs/latest/extensions)
- [Pi compaction](https://pi.dev/docs/latest/compaction)
- [Pi containerization](https://pi.dev/docs/latest/containerization)
- [Pi custom models](https://pi.dev/docs/latest/models)
- [Pi source](https://github.com/earendil-works/pi)
- [Pi subagent extension example](https://github.com/earendil-works/pi/tree/main/packages/coding-agent/examples/extensions/subagent)
- [Pi plan-mode extension example](https://github.com/earendil-works/pi/tree/main/packages/coding-agent/examples/extensions/plan-mode)
- [Octocode harness](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_HARNESS.md)
- [APPEND_SYSTEM.md starter](https://github.com/bgauryy/octocode/blob/main/docs/PI/APPEND_SYSTEM.md)
- [Octocode skills index](https://www.skills.sh/bgauryy/octocode-mcp)
- [Octocode MCP guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md)
- Octocode CLI tool reference: [GitHub tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#github-tools-reference) · [Local tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#local-code-tools-reference) · [LSP tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#lsp-tools-reference)
