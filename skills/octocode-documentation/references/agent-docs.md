Load when writing a page an agent reads, or an agent instruction file such as `AGENTS.md` or `CLAUDE.md`.

# Agent-readable pages and instruction files

## Agent-readable pages

- One idea per bullet; tables for command and path maps; short pages linking deeper ones; no intros, slogans, or restated README.
- Link the parent index, siblings, and ADRs; never "see docs".
- A command or signature only when the reader must copy it; else link the source. No code dumps (full configuration, long JSON or YAML).
- Links and bullets answer "where do I look?" and "what must I not break?".

| Information | Prefer | Why |
|---|---|---|
| Condition, action and reason | Trigger → action → check table | Agent can select one applicable row. |
| Dependency, branch, state or ordered interaction | Compact Mermaid diagram | Exposes relationships; keep conditions on edges. |
| Copyable command or exact schema | Code fence with prerequisites and expected result | Preserves executable syntax. |
| One fact or short explanation | Prose | Avoid a visual that takes longer to parse. |

## Agent instruction files

When `octocode-agentic` is available, it owns behavioral wording; this section covers index structure and link checks.

- Check how the target host loads `AGENTS.md` or `CLAUDE.md`; preserve its scope and precedence. Keep nested instructions focused on local differences.
- Draft as an index. Run `node <skill-folder>/scripts/doc-check.mjs <file> --checks links,anchors,commands`: it reports links and anchors that do not resolve and commands whose script or package script is missing. Run or inspect each command it cannot resolve, such as a tool on `PATH`.
- Sections only for non-obvious value: package manager (one line); Commands table (task, command, file-scoped test or lint); External references table (need, path); Key conventions that prevent likely mistakes.
- Symlink between hosts only when both read the same rules and the repository supports it.
- Omit welcome text, skill lists, and linter-config restatements.
