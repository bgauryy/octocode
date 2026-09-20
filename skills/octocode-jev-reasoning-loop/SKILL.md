---
name: octocode-jev-reasoning-loop
description: "Use the pure Jev CLI when bounded semantic judgment can change the next action; skip exact checks and settled decisions."
---
# Jev CLI

Inspect `octocode scheme jev --view query --compact` once; use its description and query schema as the instructions. Reuse them while current.
Run `octocode jev --input request.json --compact`. Supply short `reasoning` for why the call changes the next action, one `question` and `context: {tool, query}` for an unread tool result, or `context: {value}` for supplied evidence. Batch up to five queries; repeat context for another question. The runtime selects the model.
In this repository replace `octocode` with `node packages/octocode/out/octocode.js`.
For setup → [ojql.md](references/ojql.md). For optional examples → [jev-workflows.md](references/jev-workflows.md).
