# Octocode

`octocode` is the quick-start skill for using Octocode itself. It helps an agent pick the best available Octocode transport, run focused code or package research, read exact files, inspect symbols, check PR/history context, and look inside artifacts without turning a small lookup into a full investigation.

Use it when you want a fast, evidence-backed answer from live code or public sources.

## When to use

- Find where a local option, command, schema field, or symbol is implemented.
- Read the exact implementation behind a search hit.
- Check a GitHub repo, package, PR, commit, or file path.
- Inspect an archive, native binary, generated artifact, or text strings.
- Ask the agent to choose between Octocode MCP tools and the CLI.

Use `octocode-research` instead when the task needs root-cause investigation, code changes, review, refactor, architecture analysis, or repeated evidence loops. Use `octocode-rfc-generator` when the result should become a written proposal.

## Features

- Transport selection between exposed Octocode MCP tools and `npx octocode`.
- Cheap orientation with tree, discovery, package, repo, or symbol probes before expensive reads.
- Exact file reads using anchors such as paths, refs, line hints, match strings, package ids, PR numbers, and local clone paths.
- Local and GitHub code search, npm/package lookup, PR/commit history, AST/LSP navigation, and binary/archive inspection.
- Schema-aware raw tool calls when the quick command surface is not enough.
- Concise answers that separate leads from proof.

## How it works

The skill follows this flow:

```text
ORIENT -> SEARCH -> READ EXACT -> PROVE -> ANSWER
```

It first checks whether Octocode MCP tools are exposed. If not, it falls back to the CLI and reads live help or tool schemas before raw calls. The agent starts with the cheapest useful query, carries returned anchors forward, deep-reads only the relevant slice, and answers with evidence rather than raw search output.

## Internal flow

1. Choose transport: MCP when available, otherwise `npx octocode`.
2. Use discovery commands first: tree, path search, repo/package search, or symbol listings.
3. Promote a lead only after an exact source read, schema read, LSP result, PR/commit link, or artifact fact.
4. Preserve anchors for follow-up work: `file:line`, repo/ref, package id, PR number, or `location.localPath`.
5. Escalate to a deeper skill when the quick lookup reveals broader engineering work.

## Installation

Install the published skill:

```bash
npx octocode skill --name octocode
```

Install from a GitHub path or fork:

```bash
npx octocode skill --add bgauryy/octocode/skills/octocode
```

Octocode itself can also be used directly with `npx octocode`. MCP/editor setup is separate from skill installation.

## Benefits

- Keeps small code questions small.
- Makes the agent read schemas before guessing raw tool fields.
- Gives users proof anchors they can follow.
- Reduces noisy searches by carrying paths, refs, and matches forward.

## For developers

Keep `SKILL.md` as the lean transport and command router. Put setup/auth details in `references/octocode.md` and longer command recipes in `references/recipes.md`. When Octocode CLI flags or MCP schemas change, update the references and README examples together.
