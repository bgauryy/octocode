---
name: zod
description: "Load this skill first when writing, reviewing or testing Zod schemas or their JSON Schema output in this repo: it maps each Zod source to its generated files and lists the Zod 4 behaviors that break schemas and tests."
license: MIT
metadata:
  author: Anivar Aravind
  author_url: https://anivar.net
  source: https://github.com/anivar/zod-skill/tree/bb0620d
---

# Zod in Octocode

Every workspace resolves one Zod 4 (`rg -n '^"zod@' -A1 yarn.lock`; 4.6.7 when written). Settle an API question in `node_modules/zod/v4/classic/schemas.d.ts` (it marks deprecations) or a short `node` probe, not from memory.

## Sources and outputs

Edit the Zod source and run its generator (`$DEV` as [AGENTS.md](../../AGENTS.md#work-locally) defines it); never edit the output.

| Zod source | Output | Regenerate |
|---|---|---|
| Tool schemas in core (`../octocode-mcp-host/packages/octocode-core`) | `packages/octocode-config/contract/` | `$DEV contracts:regen` ([pipeline](../octocode-dev/docs/DEVELOPMENT.md#contract-pipeline)) |
| `packages/octocode-config/scripts/config-contract/` (`schema.ts` validates `declaration/`) | `config-contract.json` and `.schema.json`, `src/config/contract.generated.ts` (package root), `<repo>/docs/CONFIGURATION.md` | `yarn workspace @octocodeai/config generate:config-contract` ([adding config](../octocode-dev/docs/ADDING_CONFIG.md)) |
| `packages/octocode-agents-communication/src/catalog/` | `<pkg>/assets/catalog.json`, `src/runtime/catalog-types.generated.ts` | `yarn workspace @octocodeai/octocode-agents-communication types` |
| `packages/octocode-mcp-cli`: `defineCommand({ schema })` | CLI flags and help, MCP `inputSchema` | none: built at run time |

- agents-communication: Zod is a devDependency; nothing in `src/runtime`, `src/entries` or `src/hosts` may import it (`tests/catalog-source.test.ts`). A shared shape goes through `define()` in `catalog/registry.ts` (one `$defs` entry, `$ref` at each use); keywords Zod cannot state ride in `.meta()`; `catalog/json-schema.ts` maps Zod's output to the dialect `src/runtime/validation.ts` enforces.
- mcp-cli: flags come from `z.toJSONSchema(schema, { io: 'input', unrepresentable: 'any' })`. MCP SDK v2 takes the Zod schema itself as a Standard Schema; a JSON Schema tool is wrapped by `jsonSchemaStandard`.

## Zod 4 behaviors

Before writing a schema, reading its JSON Schema output or asserting on its errors, check [Zod 4 behaviors](references/zod4-behaviors.md): `toJSONSchema` modes, `.default` versus `.prefault`, object modes, records, errors, coercion and the preferred APIs.

## Tests

- Generated files are tested against their sources (`config-contract-generation.test.ts`, `tool-contract-current.test.ts`, `catalog-source.test.ts`): regenerate, never edit the expected file.
- Assert `success === false` before reading issues: an `if (!result.success)` block alone passes once the schema accepts the input. Match issue `code` and `path`, not message text.
- Vary one field per rejection case from a valid base; cover missing versus `undefined` versus `null`, both sides of each bound, and the transformed output.
- A JSON Schema snapshot misses refinements and transforms, and fixtures generated from the schema share its mistakes: keep hand-written cases.
- After a JSON Schema change, run its consumer too: CLI help and flags (`commandHelp`), MCP `tools/list`, or the catalog validator.

## Done

Stop when the source is edited, its generator has run, the generated-file test passes and the consumer shows the change. Return the result first, then the source and generated files changed, the generator and tests run with results, and any consumer left unchecked.
