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

Edit the Zod source and run its generator; never edit the output.

| Zod source | Output | Regenerate |
|---|---|---|
| Tool schemas in core (`../octocode-mcp-host/packages/octocode-core`) | `packages/octocode-config/contract/` | `$DEV contracts:regen` ([pipeline](../octocode-dev/docs/DEVELOPMENT.md#contract-pipeline)) |
| `packages/octocode-config/scripts/config-contract/` (`schema.ts` validates `declaration/`) | `config-contract.json` and `.schema.json`, `src/config/contract.generated.ts` (package root), `<repo>/docs/CONFIGURATION.md` | `yarn workspace @octocodeai/config generate:config-contract` ([adding config](../octocode-dev/docs/ADDING_CONFIG.md)) |
| `packages/octocode-agents-communication/src/catalog/` | `<pkg>/assets/catalog.json`, `src/runtime/catalog-types.generated.ts` | `yarn workspace @octocodeai/octocode-agents-communication types` |
| `packages/octocode-mcp-cli`: `defineCommand({ schema })` | CLI flags and help, MCP `inputSchema` | none: built at run time |

- agents-communication: Zod is a devDependency; nothing in `src/runtime`, `src/entries` or `src/hosts` may import it (`tests/catalog-source.test.ts`). A shared shape goes through `define()` in `catalog/registry.ts` (one `$defs` entry, `$ref` at each use); keywords Zod cannot state ride in `.meta()`; `catalog/json-schema.ts` maps Zod's output to the dialect `src/runtime/validation.ts` enforces.
- mcp-cli: flags come from `z.toJSONSchema(schema, { io: 'input', unrepresentable: 'any' })`. MCP SDK v2 takes the Zod schema itself as a Standard Schema; a JSON Schema tool is wrapped by `jsonSchemaStandard`.

## Zod 4 behaviors to check (verified on 4.6.7)

- `z.toJSONSchema` defaults to `io: 'output'`: a plain `z.object` gets `additionalProperties: false` and `.default()` keys are required; with `io: 'input'`, neither. Refinements vanish. Output-side transforms, dates, bigints and `z.custom` throw unless `unrepresentable: 'any'` (each becomes `{}`). `z.int()` emits ±`MAX_SAFE_INTEGER` bounds. A reused schema is inlined unless it is registered in a `z.registry` (with `uri`) or `reused: 'ref'` is set.
- `.default(v)` short-circuits on `undefined`: it returns `v`, an output value, without running checks or transforms. `.prefault(v)` parses `v`.
- String `min`, `max` and `length` count code points, as JSON Schema does, not UTF-16 `.length`.
- `z.object` strips unknown keys, `z.strictObject` rejects them, `z.looseObject` keeps them. `.extend()` keeps the mode; spreading `.shape` into `z.object()` resets it to strip. `.pick`, `.omit` and `.partial` throw on an object with a refinement; `.extend` keeps the refinement.
- `z.record(z.enum([...]), v)` requires every key (`z.partialRecord` does not). `.optional()` accepts an explicit `undefined`; `.exactOptional()` does not.
- Errors: pass `{ error }` (a string, or a function returning a string or `undefined` to defer). `message` is deprecated; `required_error`, `invalid_type_error` and `errorMap` are gone; a schema's `error` beats a per-parse one. Read `err.issues` (there is no `.errors`) and format with `z.prettifyError`, `z.treeifyError` or `z.flattenError`. An issue carries the raw `input` only with `reportInput: true`: keep that off for real data.
- `safeParse` does not catch an exception thrown inside `refine` or `transform`. An async check under `parse` throws `$ZodAsyncError`; use `parseAsync`.
- `z.coerce.boolean()` turns `"false"` into `true` (use `z.stringbool()`). `z.number()` rejects `Infinity`; `z.int()` accepts safe integers only.
- Prefer `z.email()`, `z.url()`, `z.uuid()`, `z.iso.datetime()` to the string methods, `z.enum(obj)` to `z.nativeEnum`, `A.extend(B.shape)` to `.merge`, `z.ZodType` to `ZodTypeAny`. `z.input` and `z.output` type the two sides of a transform.

## Tests

- Generated files are tested against their sources (`config-contract-generation.test.ts`, `tool-contract-current.test.ts`, `catalog-source.test.ts`): regenerate, never edit the expected file.
- Assert `success === false` before reading issues: an `if (!result.success)` block alone passes once the schema accepts the input. Match issue `code` and `path`, not message text.
- Vary one field per rejection case from a valid base; cover missing versus `undefined` versus `null`, both sides of each bound, and the transformed output.
- A JSON Schema snapshot misses refinements and transforms, and fixtures generated from the schema share its mistakes: keep hand-written cases.
- After a JSON Schema change, run its consumer too: CLI help and flags (`commandHelp`), MCP `tools/list`, or the catalog validator.
