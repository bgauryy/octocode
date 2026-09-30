# Config and docs audit

Load when a tool reads configuration or when checking documentation truth. Why: a knob that exists in one surface but not another, or a doc describing yesterday's behavior, silently breaks users.

## Configuration

Source of truth: `packages/octocode-config/config-contract.json`; contributor rules in `docs/ADDING_CONFIG.md` (this skill).

- List every knob the tool reads: `grep` the native config accessors in the tool module and `crates/runtime/src/config/`, plus env reads (`std::env::var`) anywhere in the tool path.
- Each knob must be: declared in `config-contract.json`; generated into `src/config/contract.generated.ts` and the native struct (`crates/runtime/build.rs`); listed in `<repo>/docs/generated/CONFIG_SETTINGS.md`; honored identically via env, `.octocoderc`, CLI, and MCP.
- Flag: env reads that bypass the contract, a knob declared but never read, duplicated resolvers (TS and Rust resolving differently), protected-key drift between TS and Rust, availability gates (clone, beta, clasify key, local tools) that differ between `$OCTO scheme --compact` and the MCP tool list.
- Check with `$OCTO config --json` and one MCP session per relevant setting.
- Generated drift: `yarn workspace @octocodeai/config check:config-contract`.

## Docs

- Tool facts (fields, defaults, views, limits, tool counts, enabled-by-default counts) in `<repo>/docs/OCTOCODE_TOOLS.md`, `<repo>/docs/TOOL_DATA_CONTRACT.md`, `docs/TOOL_QUALITY.md` (this skill), `<repo>/docs/OCTOCODE_MCP.md`, `packages/octocode/docs/OCTOCODE_CLI.md` match live `scheme` output and code.
- Stale numbers and renamed fields are the common failure — search docs for the old name after every rename.
- Remove duplicated prose across docs: keep one owner section and link to it; keep `##`/`###` anchors stable (other docs link them).
- Skill references (`skills/*/`) that show example calls must still validate against the live schema.
- Gates: `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify`, `node packages/octocode-native/scripts/check-doc-claims.cjs`. Doc repair beyond a fact fix → `octocode-documentation`.

Next: load `references/fix-and-verify.md` before editing.
