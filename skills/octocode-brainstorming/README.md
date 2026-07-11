# Octocode Brainstorming

`octocode-brainstorming` gives an agent a disciplined way to explore fuzzy ideas before anyone commits to a feature, library, workflow, or product direction.

The skill is built for the moment when a user asks, "Is this worth building?", "Has anyone already tried this?", or "What is the sharper version of this idea?" It keeps the conversation creative, but it makes the final answer answerable to evidence.

## The Problem

Early ideas are fragile. If the agent searches only for the user's first wording, it may miss better framings. If it jumps straight to enthusiasm, it can turn a vague hunch into roadmap debt. If it only lists prior art, it can still avoid the harder question: what should we do next?

This skill makes the agent diverge before it converges. It frames the idea several ways, checks evidence across the right surfaces, notices conflict, and then recommends one practical next move.

## Capabilities

- Alternative framings that prevent the first wording from anchoring the whole search.
- A surface plan that explains which sources matter: local code, web resources, GitHub, packages, and exact code reads.
- Top-resource-first prior-art mapping, so articles, docs, papers, and official sources seed repo and package research.
- Claim tracking, with confidence and next-query thinking instead of unsupported assertions.
- Cross-surface loops where web findings lead to code reads and code findings send the agent back to resources.
- Perspective review through critical, entrepreneurial, and product lenses.
- Conflict handling that concedes weak or contradictory evidence before the verdict.
- A final decision shape such as build RFC, prototype, narrow, park, or do not build.

## Operating Model

The workflow is:

```text
FRAME -> DIVERGE -> RESEARCH -> CROSS-POLLINATE -> STRESS-TEST -> SYNTHESIZE -> DECIDE
```

The agent first turns the idea into testable framings. It then researches the strongest surfaces, follows leads from one surface into another, and stress-tests the emerging thesis. The answer is not complete until the agent has stated what survived review, what was weakened, and what next step would change the decision.

## User Experience

For users, this skill feels like a product-minded technical partner. It does not merely say "yes" or "no." It names the better angle, shows which evidence supports it, marks uncertainty, and gives a next action that is small enough to be useful.

The output is a decision brief rather than code or design. When the idea becomes ready for architecture or implementation planning, it can hand off cleanly to research or RFC work.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode-brainstorming
```

## Configuration — web search keys

The skill runs web research via Tavily, Serper, and/or Exa. Put your keys in the **unified Octocode env file**, at `<octocode-home>/.env` — not in a skill-local `.env`. The Octocode home directory is resolved by `getOctocodeHome()` (`@octocodeai/config`) and is **platform-specific**, not just `~/.octocode`:

| Platform | Octocode home | `.env` path |
|---|---|---|
| macOS | `~/.octocode` | `~/.octocode/.env` |
| Linux | `${XDG_CONFIG_HOME:-~/.config}/.octocode` | `~/.config/.octocode/.env` (default) |
| Windows | `%APPDATA%\.octocode` | `%APPDATA%\.octocode\.env` |
| any platform | `$OCTOCODE_HOME` (override, if set) | `$OCTOCODE_HOME/.env` |

```bash
# example content, regardless of which path above applies to your platform:
TAVILY_API_KEY=tvly-...
SERPER_API_KEY=...
EXA_API_KEY=...
```

Never assume `~/.octocode` — always resolve the path (`npx @octocodeai/config` prints the effective home; or call `getOctocodeHome()`) before reading or writing this file, since Linux and Windows use a different default than macOS.

Get keys: [Tavily](https://app.tavily.com/) · [Serper](https://serper.dev/) · [Exa](https://dashboard.exa.ai/) · any one is enough, more gives redundancy/fallback.

**How the keys reach the skill:**
- Under the `octocode-agent` / Pi extension — `propagateOctocodeEnv` runs at session start and injects `<octocode-home>/.env` (per the table above) into `process.env`. Every subprocess (bash calls, hooks, script invocations) inherits the full env automatically.
- When the scripts are run standalone from the published build — `octocode-config.mjs` is bundled alongside each script and loads the same file directly, using the same cross-platform `getOctocodeHome()` resolution.

**Key priority (highest to lowest):** `process.env` (shell / agent session) > `<project>/.octocode/.env` (project, when trusted) > `<octocode-home>/.env` (global, per platform table above). The search scripts never overwrite an already-set value.

Verify a key is working:

```bash
node <skill_dir>/scripts/tavily-search.mjs --check
node <skill_dir>/scripts/serper-search.mjs --check
node <skill_dir>/scripts/exa-search.mjs --check
```

**Default is consolidation, not a ladder:** for real research (not a single spot-check), the skill queries every engine that passes `--check` — Tavily, Serper, and Exa each surface different results for the same query — and reconciles them (dedupe by URL, cross-confirm claims seen across engines, flag single-engine claims for verification). Engines are dispatched in parallel via `octocode-subagent` (one Web Search Scout per engine); see `references/tools.md`. DuckDuckGo (no key needed, lower quality) is the fallback only when no key validates at all, and is reported as a coverage limitation, not a silent substitute.

## Maintainer Notes

Keep the README focused on the reasoning model: divergent framing, resource-first research, surface loops, conflict concessions, and decision usefulness. Keep operational details, eval mechanics, and web adapter behavior in the agent-facing skill file and references.
