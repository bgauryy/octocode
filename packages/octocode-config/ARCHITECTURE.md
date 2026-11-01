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

Cross-language field policy has one source: `config-contract.json`, validated
against `config-contract.schema.json`. The TypeScript generator emits field
metadata, defaults, environment policy, and user documentation. Native
`build.rs` validates the same contract without Node, then emits Rust structs,
defaults, environment policy, and the generic resolver metadata the runtime
resolves settings with. Generated files are never edited by hand.

```text
config-contract.json + config-contract.schema.json
      ├── Ajv build validation → contract.generated.ts → field metadata, defaults, .env policy
      ├── documentation generation → docs/generated/CONFIG_SETTINGS.md
      └── Rust build validation → OUT_DIR/config_contract.rs → native resolver
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
            └── tool-contract/rust.ts (typify) ▶ contract/tool_types.rs ──────────┘
```

Native keeps no copy, pin, or regeneration script: cargo reruns `build.rs`
when `contract/` changes, and the build fails if the files disagree on the
fingerprint. `check:core-contract-sync:published` compares `contract/` with the
npm-published core pinned at the repo root and gates releases.

**Naming is owned by core.** Each tool yields `<Tool>Query` (one row),
`<Tool>Input` (bulk envelope), and `<Tool>Output` (result envelope). A schema
core names — `.meta({ id })` or `.meta({ title })` — becomes one global type
(`ChunkType`, `MinifyMode`, `ResponseScope`, `AstRule`, …); give a vocabulary
shared across tools a title in core. Everything else is named by position
(`LocalSearchQueryCaseMode`), so an unrelated contract change never renames a
type. Two shapes under one name, or a recursive schema core left unnamed, fail
generation.

**Faithful Rust shapes.** typify drops a string `const` and flattens `anyOf`,
so the bundle states both precisely: a string `const` becomes a one-value
`enum` (enforced), and `anyOf` over closed objects becomes `oneOf` (an enum).
When branches share a discriminator value, variants are named from it
(`AstSearchQueryMatchPattern` / `…MatchRule`). Maps are `BTreeMap`
(deterministic). Boolean `const`s remain a contract-validator check.

The Rust header carries the core contract fingerprint and pins the bundle's SHA-256,
so `--check` (run by `build` and `lint`) detects staleness without cargo.
Regenerating needs `cargo install cargo-typify --version 0.8.0 --locked`. Where core's
output schema is open (`unknown[]` payloads), the generated type is open too:
tightening a payload shape is a core schema change, never a hand-written type.

## Invariants

- Importing the library performs no environment mutation.
- For each environment key: non-empty process value → workspace `.octocode/.env` → home `.env`. GitHub and classification credentials follow this order.
- Workspace dotenv loads by default. Node hosts may explicitly opt out with `trusted:false`; native `trustedProject` remains separate permission for executable LSP configuration.
- Process bootstrap keys stay blocked in both files. Home-only keys are skipped in a workspace `.env`, except the narrowing values the contract allows. A present-but-blank process `OCTOCODE_CLASSIFICATION_API` stays an explicit opt-out.
- Credential source priority applies across aliases before alias priority within the same source; token discovery, refresh, and storage remain native responsibilities.
- Parsing is deterministic and does not execute shell syntax.
- Consumers receive explicit environment objects where isolation matters.
- The root entry uses Node builtins only, so public packages and standalone skills can bundle it without importing another policy owner; only `./schema` and `./chrome` import core. Ajv is build/test-only and is absent from `dist`.

## Distribution

Consumers declare it as a devDependency so workspace build ordering and declaration generation stay deterministic, and bundle it into their output.
