# Config architecture

`@octocodeai/config` is the zero-runtime-dependency, independently publishable owner of Octocode environment and product-home policy. Public packages may depend on it normally or bundle it for standalone delivery, but they must not reimplement its rules.

## Data flow

```text
process environment
      ├── home .env / .octocoderc
      └── workspace .octocode/.env
                 │
                 ▼
      parse → trust policy → resolved config
                 │
                 ├── CLI / MCP runtime surfaces
                 ├── Pi and communication
                 └── injected standalone skill helper
```

## Declarative contract

Cross-language field policy has one source: `config-contract.json`, validated
against `config-contract.schema.json`. The TypeScript generator emits public
input/resolved types, defaults, environment policy, generic-interpreter
metadata, and user documentation. Native `build.rs` validates the same contract
without Node, then emits Rust structs, defaults, environment policy, and the
same interpreter metadata. Generated files are never edited by hand.

```text
config-contract.json + config-contract.schema.json
      ├── Ajv build validation → contract.generated.ts → generic TS config
      ├── documentation generation → docs/generated/CONFIG_SETTINGS.md
      └── Rust build validation → OUT_DIR/config_contract.rs → generic native config
```

Resolver and validator source files own language mechanics only. They must not
contain per-setting paths, defaults, environment names, bounds, enum sets, or
known-key lists.

## Ownership

- `home` owns `OCTOCODE_HOME` and platform-default resolution.
- `env` owns parsing, precedence, propagation, and diagnostics.
- `config` owns structured `.octocoderc` loading.
- `policy` owns protected keys and project-level override restrictions.
- The CLI exposes inspection only; it does not add a second configuration model.
- `@octocodeai/octocode-core` authors every tool contract, Zod schema,
  description, and capability-gated schema variants. The `./schema` and `./mcp` subpaths re-export it so
  interfaces import contracts from one place; the root `.` entry never imports
  core.
- This package owns the tool input/output **types** for every language (see
  below). No surface hand-writes a tool wire type in TypeScript or Rust.

## Tool types

This package is the **only** tool-contract generator. `yarn contracts:regen`
(repo root: refresh the core copy, then `generate:tool-contract`) is the whole
change for every consumer:

```text
core Zod schemas ── buildEnforcementContractIr ──▶ contract/tool-contract.json ─┐
      ├── buildNativeParityFixtures ─────────────▶ contract/contract-fixtures.json├─ embedded by
      ├── provenance (core version, fingerprint, sha) ▶ contract/provenance.json ─┤  octocode-native
      └── tool-contract/bundle.ts ──▶ contract/tool-types.schema.json             │  build.rs, in place
            ├── tool-contract/typescript.ts ──▶ src/contracts/toolTypes.generated.ts│
            │        (./schema: <Tool>Query/Input/Output, ToolQuery<N>, …)        │
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

Both files carry the core contract fingerprint; the Rust header also pins the bundle's SHA-256,
so `--check` (run by `build` and `lint`) detects staleness without cargo.
Regenerating needs `cargo install cargo-typify --version 0.8.0 --locked`. Where core's
output schema is open (`unknown[]` payloads), the generated type is open too:
tightening a payload shape is a core schema change, never a hand-written type.

## Invariants

- Importing the library performs no environment mutation.
- For each environment key: non-empty process value → workspace `.octocode/.env` → home `.env`. GitHub and classification credentials follow this order.
- Workspace dotenv loads by default. Node hosts may explicitly opt out with `trusted:false`; native `trustedProject` remains separate permission for executable LSP configuration.
- Process bootstrap keys remain blocked in both files; every declared product setting accepts both files. A present-but-blank process `OCTOCODE_CLASSIFICATION_API` remains an explicit opt-out.
- Credential source priority applies across aliases before alias priority within the same source; token discovery, refresh, and storage remain native responsibilities.
- Parsing is deterministic and does not execute shell syntax.
- Consumers receive explicit environment objects where isolation matters.
- The published package remains zero-runtime-dependency so it can be bundled into public packages and standalone skills without importing another policy owner. Ajv is build/test-only and is absent from `dist`.

## Distribution

The package is public and versioned independently. `octocode` and the Pi extension bundle it for self-contained delivery; build-only consumers must still declare it so workspace ordering and declaration generation are deterministic.
