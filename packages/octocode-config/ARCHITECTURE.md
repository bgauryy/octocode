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
- `@octocodeai/octocode-core` owns every tool contract, Zod schema, description,
  and capability-gated schema variant (such as search `semanticRerank`). The
  `./schema` and `./mcp` subpaths only re-export core so interfaces import
  contracts from one place; the root `.` entry never imports core.

## Invariants

- Importing the library performs no environment mutation.
- Project configuration cannot replace protected credentials or security controls.
- Parsing is deterministic and does not execute shell syntax.
- Consumers receive explicit environment objects where isolation matters.
- The published package remains zero-runtime-dependency so it can be bundled into public packages and standalone skills without importing another policy owner. Ajv is build/test-only and is absent from `dist`.

## Distribution

The package is public and versioned independently. `octocode` and the Pi extension bundle it for self-contained delivery; build-only consumers must still declare it so workspace ordering and declaration generation are deterministic.
