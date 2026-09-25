# octocode-dev

Repo-internal skill for auditing and hardening Octocode's own tools end to end: core contract (schema, descriptions, MCP/CLI instructions) → native implementation and data flow → output shape and pagination → agent workflow hints → config and docs → cleanup.

Entry point: [`SKILL.md`](SKILL.md).

| File | Purpose |
|---|---|
| `scripts/tool-inventory.mjs` | Per-tool map: native module, evidence files, variants, zero-hit/unclassified/undescribed input fields |
| `references/surface-map.md` | Where every layer of a tool lives |
| `references/contract-audit.md` | Schema, description, instruction checks |
| `references/implementation-audit.md` | Schema↔code alignment, data flow, efficiency, caching |
| `references/output-audit.md` | Pagination, truncation, redundancy, rigidity |
| `references/workflow-audit.md` | Agent chaining, hints, `next.*` |
| `references/config-docs-audit.md` | Config across surfaces, docs truth |
| `references/fix-and-verify.md` | Core→native→CLI/MCP landing order and verification gate |
| `assets/audit-report.md` | Report template |

Run the inventory from anywhere inside the monorepo:

```bash
node .agents/skills/octocode-dev/scripts/tool-inventory.mjs            # all tools
node .agents/skills/octocode-dev/scripts/tool-inventory.mjs localFetch --json
```
