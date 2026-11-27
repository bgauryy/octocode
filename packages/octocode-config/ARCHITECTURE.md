# Config architecture

`@octocodeai/config` owns Octocode environment and product-home policy, the configuration contract, and the generated tool contract. It is a private workspace package, never published: public packages bundle it at build time (as a `workspace:*` devDependency) and must not reimplement its rules.

## Data flow

```text
process environment + home .env + workspace .octocode/.env
                 │
                 ▼
      parse → protected-key policy → environment   (TypeScript: `.env` only)
                 │
                 └── CLI launcher / MCP server → native runtime (also reads both .octocoderc files)
```

## Declarative contract

Cross-language field policy has one source: the Zod declaration in
`scripts/config-contract/declaration/`, shaped by `scripts/config-contract/schema.ts`.
The generator validates it with that schema and emits `config-contract.json` and
`config-contract.schema.json` (`z.toJSONSchema`), field
metadata, defaults, environment policy, and user documentation. A test fails when
any generated file drifts from the declaration. Native
`build.rs` reads the emitted JSON without Node, then emits Rust structs,
defaults, environment policy, and the generic resolver metadata the runtime
resolves settings with. Generated files are never edited by hand.

```text
declaration/ (Zod schema.ts validates)
      ├── emit → config-contract.json + config-contract.schema.json
      ├── contract.generated.ts → field metadata, defaults, .env policy
      ├── documentation generation → the settings table in docs/CONFIGURATION.md
      └── config-contract.json → Rust build → OUT_DIR/config_contract.rs → native resolver
```

Hand-written source owns language mechanics only. It must not contain
per-setting paths, defaults, environment names, bounds, enum sets, or
known-key lists.

## Ownership

- `src/home.ts` owns `OCTOCODE_HOME` and the platform default.
- `src/dotenv.ts` owns `.env` parsing, precedence, propagation, protected keys, and workspace override restrictions.
- `.octocoderc` parsing and setting resolution are native (`npx octocode config` shows the result).
- The root export exposes generated `CONFIG_FIELDS` and `DEFAULT_CONFIG` for configuration editors.
- Native management uses the same metadata to validate edits and redact secret values.
- The CLI transports management requests without adding a second configuration model.
- `@octocodeai/octocode-core` authors every tool contract, Zod schema,
  description, and capability-gated schema variants. The `./schema` (research
  tools) and `./chrome` (Chrome DevTools) subpaths re-export it so interfaces
  import contracts from one place; the root `.` entry never imports core.
- This package owns the generated Rust tool **types** (see below). No surface
  hand-writes a tool wire type in TypeScript or Rust; TypeScript uses core's Zod
  schemas.

## Tool types

This package is the **only** tool-contract generator. `node skills-dev/octocode-dev/scripts/dev.mjs contracts:regen`
(repo root: refresh the core copy, then `generate:tool-contract`) is the whole
change for every consumer:

```text
core Zod schemas ── buildEnforcementContractIr ──▶ contract/tool-contract.json ─┐
      ├── buildNativeParityFixtures ─────────────▶ contract/contract-fixtures.json├─ embedded by
      ├── provenance (core version, fingerprint, sha) ▶ contract/provenance.json ─┤  octocode-native
      └── tool-contract/bundle.ts ──▶ contract/tool-types.schema.json             │  build.rs, in place
            └── tool-contract/rust.ts (typify) ▶ contract/tool_types.rs ──────────┤
                                               contract/tool_types_base.rs ─────┘
```

Native keeps no copy, pin, or regeneration script: cargo reruns `build.rs`
when `contract/` changes, and the build fails if the files disagree on the
fingerprint. `check:core-contract-sync:published` compares `contract/` with the
npm-published core pinned at the repo root and gates releases.

**Naming is owned by core.** Each tool yields `<Tool>Query` (one row),
`<Tool>Input` (bulk envelope), and `<Tool>Output` (result envelope). A schema
core names — `.meta({ id })` or `.meta({ title })` — becomes one global type
(`ChunkType`, `MinifyMode`, `ResponseScope`, `AstRule`, …); give a vocabulary
shared across tools a title in core. Other shapes are named by position
(`LocalSearchQueryCaseMode`). In the Rust projection, repeated anonymous string
constraints share a definition named from those constraints: the same validation
compiles once instead of once per field. Named core types remain distinct, and
use-site defaults remain at their fields. Native callers infer anonymous field
types rather than depending on their generated names. Unrelated contract changes
do not rename these types. Two shapes under one core name, or a recursive schema
core left unnamed, fail generation.

**Faithful Rust shapes.** typify drops a string `const` and flattens `anyOf`,
so the bundle states both precisely: a string `const` becomes a one-value
`enum` (enforced), and `anyOf` over closed objects becomes `oneOf` (an enum).
When branches share a discriminator value, variants are named from it
(`AstSearchQueryMatchPattern` / `…MatchRule`). Maps are `BTreeMap`
(deterministic). Boolean `const`s remain a contract-validator check.

**Derives by role.** Every generated type derives `Serialize`. `Deserialize` is
derived only where something decodes: what a tool's `*Query` or `*Input`
reaches, what native source names (saved pages and plans, rows checked against
their type), and what no tool schema reaches; a type only a `*Output` reaches is
encode-only (`rust-serde.ts`). The types native names through
`octocode_contract_base::` go to `contract/tool_types_base.rs` (crate
`contract-base`, which the GitHub crate depends on), with what they reach and the
shared conversion error; `tool_types.rs` re-exports them, so each has one
definition.

Both Rust headers carry the core contract fingerprint; `tool_types.rs` also pins the bundle's SHA-256,
and `--check` regenerates and compares the complete body as well as the header.
Both config and tool generation write only changed files through `scripts/generated-files.ts`,
preserving Cargo's freshness checks when their contracts are unchanged.
Regenerating needs `cargo install cargo-typify --version 0.8.0 --locked`. Where core's
output schema is open (`unknown[]` payloads), the generated type is open too:
tightening a payload shape is a core schema change, never a hand-written type.

## Invariants

- Importing the library performs no environment mutation.
- For each environment key: non-empty process value → workspace `.octocode/.env` → home `.env`. GitHub and classification credentials follow this order.
- Workspace dotenv loads by default. Node hosts may explicitly opt out with `trusted:false`; native `trustedProject` remains separate permission for executable LSP configuration.
- Process bootstrap keys stay blocked in both files. Home-only keys are skipped in a workspace `.env`, except the narrowing values the contract allows. A present-but-blank process `OCTOCODE_TYPESAFE_KEY` stays an explicit opt-out.
- Credential source priority applies across aliases before alias priority within the same source; token discovery, refresh, and storage remain native responsibilities.
- Parsing is deterministic and does not execute shell syntax.
- Consumers receive explicit environment objects where isolation matters.
- The root entry uses Node builtins only, so public packages and standalone skills can bundle it without importing another policy owner; only `./schema` and `./chrome` import core. The config-contract generator's Zod declaration and the tests' Ajv are build/test-only; the root entry imports neither.

## Distribution

Consumers declare it as a devDependency so workspace build ordering and declaration generation stay deterministic, and bundle it into their output.
