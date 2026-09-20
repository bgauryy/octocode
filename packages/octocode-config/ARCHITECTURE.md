# Config architecture

`@octocodeai/config` is the zero-runtime-dependency, independently publishable owner of Octocode environment and product-home policy. Public packages may depend on it normally or bundle it for standalone delivery, but they must not reimplement its rules.

## Data flow

```text
process environment
      ├── global .env / .octocoderc
      └── project .env / .octocoderc
                 │
                 ▼
      parse → trust policy → resolved config
                 │
                 ├── CLI / MCP runtime surfaces
                 ├── Pi and Awareness
                 └── injected standalone skill helper
```

## Shared constants

Cross-language config constants have one value source:
`shared-constants.json`. A build-only Zod schema validates its shape and policy
invariants. The config generator emits literal TypeScript exports, while the
native runtime's `build.rs` emits Rust constants from the same JSON. Generated
files are never edited by hand.

```text
shared-constants.json
      ├── Zod validation → sharedConstants.generated.ts → TypeScript config
      └── Rust build.rs  → OUT_DIR/shared_constants.rs  → native config
```

## Ownership

- `home` owns `OCTOCODE_HOME` and platform-default resolution.
- `env` owns parsing, precedence, propagation, and diagnostics.
- `config` owns structured `.octocoderc` loading.
- `policy` owns protected keys and project-level override restrictions.
- The CLI exposes inspection only; it does not add a second configuration model.
- `@octocodeai/octocode-core` owns tool contracts and Zod tool schemas. Config
  neither imports nor re-exports core; joining those packages would couple
  environment policy to the independently versioned public tool contract.

## Invariants

- Importing the library performs no environment mutation.
- Project configuration cannot replace protected credentials or security controls.
- Parsing is deterministic and does not execute shell syntax.
- Consumers receive explicit environment objects where isolation matters.
- The published package remains zero-runtime-dependency so it can be bundled into public packages and standalone skills without importing another policy owner. Zod is build-only and is absent from `dist`.

## Distribution

The package is public and versioned independently. `octocode` and the Pi extension bundle it for self-contained delivery; build-only consumers must still declare it so workspace ordering and declaration generation are deterministic.
