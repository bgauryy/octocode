---
name: octocode
description: "Use when a task needs Octocode-powered code research: local or GitHub search, exact file reads, npm/package lookup, PR or commit history, AST/LSP navigation, binary/archive inspection, or raw Octocode tool calls. Trigger when the user asks to use Octocode, inspect code with evidence, or choose MCP vs CLI transport."
---

# Octocode

Use Octocode for evidence-driven code research across local files, GitHub, npm, PR/history, archives, and LSP/AST semantics. Read schemas/help before raw calls and carry returned paths, refs, lines, pages, and `location.localPath` forward.

Flow: `ORIENT -> SEARCH -> READ EXACT -> PROVE -> ANSWER`.

## Transport

Prefer Octocode MCP tools when exposed. Otherwise use `npx octocode`; run `npx octocode --help`, `<command> --help`, or `tools <name> --scheme` before uncertain inputs. If neither transport works, say so and continue with degraded confidence only when Octocode is not essential.

## Quick Map

- Tree/files: `npx octocode search <path|owner/repo> --tree`; `search <query> <path> --search path`.
- Text/code: `npx octocode search <term> <path|owner/repo> --view discovery`.
- Exact read: `npx octocode search <file|owner/repo/path> --match-string <anchor> --content-view exact`.
- Symbols/LSP: `npx octocode search <file> --symbols`; `--op references|callers|callees|definition --symbol <name> --line <hint>`.
- AST: `npx octocode search <path> --pattern '<shape>' --lang <lang>` or `--rule '<yaml>'`.
- Prior art: `npx octocode search <query> --target repositories|packages`.
- PR/history: `npx octocode search owner/repo#123 --target pullRequests`; `--target commits`.
- Artifacts: `npx octocode search <file> --target artifacts --inspect|--list|--strings`; `npx octocode unzip <archive>`.
- Remote as local: `npx octocode search <term> <subdir> --repo owner/repo --json`; then continue on returned local path.

Use `--json` when another step depends on machine anchors; use `--compact` for triage. Raw tools: `npx octocode tools`, `tools <name> --scheme`, then `tools <name> --queries '<json>'`.

## Reference Map

- `references/octocode.md` — when choosing transport, installing/configuring Octocode, checking auth, or explaining CLI/MCP behavior.
- `references/recipes.md` — when a task needs short command sequences beyond the quick map.

## Installation

For users: install or run the CLI with `npx octocode`. For skill installation use the Octocode skill CLI, for example:

```bash
npx octocode skill --name octocode
```

## Escalation

Use `octocode-research` for large investigations, reviews, refactors, AST/LSP-heavy work, repeated loops, or code changes. Use `octocode-rfc-generator` for formal proposals.

Bad input exits `2`; not found `3`; auth `4`; tool error `5`; rate limit `7`. Check `npx octocode auth status --json` or `status --json` and use available `GITHUB_TOKEN`, `OCTOCODE_TOKEN`, or `GH_TOKEN`.
