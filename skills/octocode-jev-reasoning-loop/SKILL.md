---
name: octocode-jev-reasoning-loop
description: "Use the pure Jev CLI when bounded semantic judgment can change the next action; skip exact checks and settled decisions."
---
# Jev CLI

Inspect `octocode tools jev --scheme --scheme-view query --json --compact` once; use its description and query schema as the instructions. Reuse them while current.
Run `octocode tools jev --input request.json --json --compact`. Supply `state`, `questions`, and optional `sources` for unread local/GitHub files; the runtime loads content and selects the model.
In this repository replace `octocode` with `node packages/octocode/out/octocode.js`.
For setup → [ojql.md](references/ojql.md). For optional examples → [jev-workflows.md](references/jev-workflows.md).
