Load when judging a tool's schema, descriptions, instructions, config knobs or documentation truth.

# Contract, config and docs audit

## Input schema
- One job per field: flag duplicate intents (`limit` vs `pageSize`), aliases kept after a rename, booleans that should be one enum.
- Defaults live once: schema `default` and native rules agree; flag a default re-applied in Rust.
- Every `maximum`/`maxItems` maps to a runtime or provider limit. A bound the runtime never hits is noise; a runtime cap with no schema bound is a silent clamp.
- Discriminators (`operation`, `type`) reject cross-variant fields.
- Required matches execution needs; inferred fields are optional.
- Names match sibling tools (`path`, `limit`, `cursor`, `responseChars`); flag one-offs.

## Descriptions and instructions
- Description is `<when>…</when><workflow>…</workflow>`: `<when>` names triggers and when a sibling fits better; `<workflow>` says which outputs feed it, where results go and what empty means. No `next.*` names.
- A field description adds what the name cannot (unit, default effect, interaction). Delete ones that restate the name; add one only where the inventory flags an ambiguous undescribed field.
- No claim the implementation cannot back (languages, views, limits); verify in code or a live call.
- Instructions are one core base (`MCP_INSTRUCTIONS`) plus a block per optional tool (`MCP_TOOL_INSTRUCTIONS`); the native CLI appends its `<cli>` notes. Flag tool guidance hand-written in `packages/octocode-mcp` or `packages/octocode`.
- Each sentence changes an agent decision; keep a rule at its narrowest owner. Rewrites go through `octocode-agentic`; context cost: core `publishedSurfaceBudget.test.ts`.

## Config and docs
Source of truth: the Zod declaration in `packages/octocode-config/scripts/config-contract/declaration/` (emits `config-contract.json`); rules in `docs/ADDING_CONFIG.md`.
- List every knob the tool reads: native config accessors in the tool module and `crates/runtime-base/src/config/`, plus `std::env::var` reads in the tool path.
- Each knob is declared there, generated into `src/config/contract.generated.ts` and the native struct (`crates/runtime-base/build.rs`), listed in `<repo>/docs/CONFIGURATION.md`, and honored identically via env, `.octocoderc`, CLI and MCP.
- Flag env reads that bypass the contract, declared knobs never read, differing TS and Rust resolvers, and protected-key drift. Each surface follows its own availability policy: MCP exposure settings (clone, beta, local tools, tool lists) do not remove CLI commands; shared capability requirements (the classification key) apply to both. Verify via `$OCTO schema` and the MCP tool list.
- Check with `$OCTO config --json` and one MCP session per setting. Drift: `yarn workspace @octocodeai/config check:config-contract`.
- Tool facts in the surface-map Docs row match live `schema` output; after a rename, search docs for the old name. One owner section per fact; keep anchors stable; `skills/*/` example calls validate against the live schema.
- Gate: `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify`. Larger doc repair: `octocode-documentation`.

Record per finding: layer, field or sentence, evidence (core `file:line` + live `schema` output), proposed change, owner repo.

Next: `references/implementation-audit.md`.
