# Octocode

`octocode` is the quick entry skill for code research through Octocode. It helps an agent choose the best available transport, then use Octocode MCP tools or the CLI for focused search, exact file reads, symbol navigation, repository lookup, package lookup, PR/history checks, and artifact inspection.

Use it when you want a fast answer grounded in code evidence, not a full engineering investigation.

## How it works

The skill first chooses the best live transport: Octocode MCP tools when they are available, or `npx octocode` when the CLI is the right fit for the running environment. It orients with the cheapest useful query, carries returned anchors forward, reads exact files or schemas only when needed, and answers with proof rather than a raw search dump.

## Good asks

- "Use Octocode to find where this option is parsed."
- "Search the repo for this symbol and read the exact implementation."
- "Check whether this package exists and where its source lives."
- "Look up the PR or commit history for this behavior."
- "Inspect this archive or binary enough to tell me what it contains."

## What you get

- The transport used: MCP or `npx octocode`.
- Cheap orientation before exact reads.
- File paths, line numbers, refs, package ids, or PR numbers carried forward.
- A short answer that separates leads from proof.
- A next action when the quick lookup exposes a larger task.

## Use another skill when

- The task needs a review, refactor, architecture assessment, or implementation: use `octocode-engineer`.
- The answer needs iterative convergence across several tool calls: use `octocode-loop`.
- The request is broad technical research with no code edits: use `octocode-research`.
- The user needs an RFC or proposal: use `octocode-rfc-generator`.

## User value

This skill keeps small code questions small. It avoids a long playbook, reads live Octocode schemas when raw tools are needed, and returns only the evidence a user needs to trust the answer.
