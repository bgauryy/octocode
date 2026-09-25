# Contract audit

Load when judging a tool's input schema, descriptions, and MCP/CLI instructions. Why: agents only see this text; a wrong or bloated word misroutes every call.

## Input schema

- Every field has one job. Flag two fields that express the same intent (`limit` vs `pageSize`, `maxResults` vs `maxFiles`), aliases kept after a rename, and booleans that should be one enum.
- Defaults live once: schema `default` and native `defaults`/rules must agree; flag a default re-applied in Rust.
- Bounds are real: every `maximum`/`maxItems` maps to a runtime or provider limit; a bound the runtime never hits is noise, a runtime cap with no schema bound is a silent clamp.
- Variants and discriminators (`operation`, `analysis`, `type`, …) reject cross-variant fields; a field valid in only one branch must not appear in the shared shape.
- Required vs optional matches what execution needs; anything the runtime infers stays optional.
- Names match the domain and sibling tools (`path`, `pageSize`, `page`, `matchPage`) — flag one-off spellings.

## Descriptions

- `shortDescription` states when to pick this tool over its siblings in one clause.
- Field descriptions add what the name cannot: unit, default effect, interaction. Delete descriptions that restate the name; add one only where the inventory flags a genuinely ambiguous undescribed field.
- No claims the implementation cannot back (supported languages, views, limits). Verify each claim against code or a live call.

## Instructions (MCP + CLI)

- Built from core (`buildMcpInstructions(enabledToolNames)` / `buildCliToolContext`) — flag any tool guidance hand-written in `packages/octocode-mcp` or `packages/octocode`.
- Per enabled subset: text for a disabled tool must not render. Check `$OCTO scheme --compact` with and without optional tools enabled.
- Each sentence changes an agent decision; remove duplicates between instructions and per-tool descriptions (keep the rule at the narrowest owner).
- Rewrites go through `octocode-prompt-optimizer`; measure context cost with core `agentContextBudgets.test.ts`.

## Record

Per finding: layer, field/sentence, evidence (core file:line + live `scheme` output), proposed change, owner repo.

Next: load `references/implementation-audit.md` to prove each field against code.
