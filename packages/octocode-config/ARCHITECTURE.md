# Config architecture

`@octocodeai/config` is the zero-dependency, independently publishable owner of Octocode environment and product-home policy. Public packages may depend on it normally or bundle it for standalone delivery, but they must not reimplement its rules.

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

## Ownership

- `home` owns `OCTOCODE_HOME` and platform-default resolution.
- `env` owns parsing, precedence, propagation, and diagnostics.
- `config` owns structured `.octocoderc` loading.
- `policy` owns protected keys and project-level override restrictions.
- The CLI exposes inspection only; it does not add a second configuration model.

## Invariants

- Importing the library performs no environment mutation.
- Project configuration cannot replace protected credentials or security controls.
- Parsing is deterministic and does not execute shell syntax.
- Consumers receive explicit environment objects where isolation matters.
- The package remains zero-dependency so it can be bundled into public packages and standalone skills without importing another policy owner.

## Distribution

The package is public and versioned independently. `octocode` and the Pi extension bundle it for self-contained delivery; build-only consumers must still declare it so workspace ordering and declaration generation are deterministic.
