# Config architecture

`@octocodeai/config` is the zero-runtime-dependency, independently publishable owner of Octocode environment and product-home policy. Public packages may depend on it normally or bundle it for standalone delivery, but they must not reimplement its rules.

## Data flow

```text
process environment
      ├── home .env / .octocoderc
      └── trusted project .octocode/.env
                 │
                 ▼
      parse → trust policy → resolved config
                 │
                 ├── CLI / MCP runtime surfaces
                 ├── Pi and Awareness
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
  description, and capability-gated schema variant (such as search
  `semanticRerank`). The `./schema` and `./mcp` subpaths re-export it so
  interfaces import contracts from one place; the root `.` entry never imports
  core.
- This package owns the tool input/output **types** for every language (see
  below). No surface hand-writes a tool wire type in TypeScript or Rust.

## Tool types

One derivation path produces every tool input/output type:

This package is the **only** tool-contract generator. `yarn contracts:regen`
(repo root: refresh the core copy, then `generate:tool-contract`) is the whole
change for every consumer:

```text
core Zod schemas ── buildEnforcementContractIr ──▶ contract/tool-contract.json ─┐
      ├── buildNativeParityFixtures ─────────────▶ contract/contract-fixtures.json├─ embedded by
      ├── provenance (core version, fingerprint, sha) ▶ contract/provenance.json ─┤  octocode-native
      └── bundle ──▶ contract/tool-types.schema.json                              │  build.rs, in place
            ├── json-schema-to-typescript ──▶ src/contracts/toolTypes.generated.ts│
            │        (./schema: <Tool>Query/Input/Output, ToolQuery<N>, …)        │
            └── cargo-typify 0.8.0 ─────────▶ contract/tool_types.rs ─────────────┘
```

Native keeps no copy, pin, or regeneration script: cargo reruns `build.rs`
when `contract/` changes, and the build fails if the files disagree on the
fingerprint. `scripts/check-core-contract-sync.cjs` (`--published` for the
npm-published core) gates releases.

`scripts/generate-tool-contract.ts` names each tool's `Query` (one row),
`Input` (bulk envelope), and `Output` (result envelope), shares identical
output `$defs` as `Shared*`, and lifts a property enum used by several tools
(`chunkType`, `minify`, `caseMode`, …) into one named type. Both files carry
the core contract fingerprint; the Rust header also pins the bundle's SHA-256,
so `--check` (run by `build` and `lint`) detects staleness without cargo.
Regenerating needs `cargo install cargo-typify --version 0.8.0 --locked`. Where core's
output schema is open (`unknown[]` payloads), the generated type is open too:
tightening a payload shape is a core schema change, never a hand-written type.

## Invariants

- Importing the library performs no environment mutation.
- Project configuration cannot replace protected credentials or security controls.
- Parsing is deterministic and does not execute shell syntax.
- Consumers receive explicit environment objects where isolation matters.
- The published package remains zero-runtime-dependency so it can be bundled into public packages and standalone skills without importing another policy owner. Ajv is build/test-only and is absent from `dist`.

## Distribution

The package is public and versioned independently. `octocode` and the Pi extension bundle it for self-contained delivery; build-only consumers must still declare it so workspace ordering and declaration generation are deterministic.
