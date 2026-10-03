# Contract, config, and docs audit

Load when judging a tool's input schema, descriptions, MCP/CLI instructions, configuration knobs, or documentation truth. Agents see only this text; a wrong or bloated word misroutes every call.

## Input schema
- Every field has one job. Flag two fields with the same intent (`limit` vs `pageSize`, `maxResults` vs `maxFiles`), aliases kept after a rename, and booleans that should be one enum.
- Defaults live once: schema `default` and native `defaults`/rules agree. Flag a default re-applied in Rust.
- Every `maximum`/`maxItems` maps to a runtime or provider limit. A bound the runtime never hits is noise; a runtime cap with no schema bound is a silent clamp.
- Discriminators (`operation`, `analysis`, `type`, …) reject cross-variant fields. A field valid in one branch must not appear in the shared shape.
- Required vs optional matches what execution needs; anything the runtime infers stays optional.
- Names match the domain and sibling tools (`path`, `pageSize`, `page`, `matchPage`). Flag one-off spellings.

## Descriptions and instructions
- `shortDescription` states in one clause when to pick this tool over its siblings.
- A field description adds what the name cannot: unit, default effect, interaction. Delete descriptions that restate the name; add one only where the inventory flags an ambiguous undescribed field.
- No claim the implementation cannot back (languages, views, limits). Verify each against code or a live call.
- Instructions come from core (`buildMcpInstructions(enabledToolNames)` / `buildCliToolContext`). Flag tool guidance hand-written in `packages/octocode-mcp` or `packages/octocode`.
- Text for a disabled tool must not render. Check `$OCTO scheme --compact` with and without optional tools enabled.
- Each sentence changes an agent decision. Keep a rule at its narrowest owner, not in both instructions and descriptions. Rewrites go through `octocode-agentic-prompts`; measure context cost with core `agentContextBudgets.test.ts`.

## Config and docs
Config source of truth: `packages/octocode-config/config-contract.json`; contributor rules: `docs/ADDING_CONFIG.md`.
- List every knob the tool reads: native config accessors in the tool module and `crates/runtime/src/config/`, plus `std::env::var` reads anywhere in the tool path.
- Each knob is declared in `config-contract.json`, generated into `src/config/contract.generated.ts` and the native struct (`crates/runtime/build.rs`), listed in `<repo>/docs/generated/CONFIG_SETTINGS.md`, and honored identically via env, `.octocoderc`, CLI, and MCP.
- Flag env reads that bypass the contract, declared knobs never read, TS and Rust resolvers that differ, protected-key drift, and availability gates (clone, beta, clasify key, local tools) that differ between `$OCTO scheme --compact` and the MCP tool list.
- Check with `$OCTO config --json` and one MCP session per relevant setting. Generated drift: `yarn workspace @octocodeai/config check:config-contract`.
- Tool facts (fields, defaults, views, limits, tool and enabled-by-default counts) in each doc of the `references/surface-map.md` Docs row match live `scheme` output and code. After every rename, search docs for the old name.
- Keep one owner section per fact and link to it; keep `##`/`###` anchors stable. Example calls in `skills/*/` still validate against the live schema.
- Gates: `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify`, `node packages/octocode-native/scripts/check-doc-claims.cjs`. Doc repair beyond a fact fix → `octocode-documentation`.

Record per finding: layer, field or sentence, evidence (core `file:line` + live `scheme` output), proposed change, owner repo.

Next: load `references/implementation-audit.md` to prove each field against code.
