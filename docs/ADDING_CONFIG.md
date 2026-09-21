# Adding configuration to Octocode

This guide explains how to add settings and credentials without creating TypeScript/Rust drift.

## Architecture

`packages/octocode-config/config-contract.json` is the only declaration of configuration field policy. It owns:

- file paths and section membership;
- input and resolved types;
- defaults and inherited defaults;
- environment names and alias priority;
- ranges, enum values, URL/path semantics, and unknown-key membership;
- dotenv trust (`all`, `home`, or `never`);
- credential exclusion from `ResolvedConfig`;
- user-facing descriptions and generated reference data.

The contract is validated by `config-contract.schema.json` and then consumed through two build paths:

```text
packages/octocode-config/config-contract.json
                 │
                 ├─ TypeScript generator
                 │    ├─ src/config/contract.generated.ts
                 │    │    types · defaults · field metadata · env policy
                 │    └─ docs/generated/CONFIG_SETTINGS.md
                 │
                 └─ Rust build.rs
                      └─ $OUT_DIR/config_contract.rs
                           structs · defaults · field metadata · env policy

contract metadata → generic TypeScript resolver/validator
contract metadata → generic Rust resolver/validator
```

The interpreters contain language mechanics—reading JavaScript objects or `serde_json::Value`, parsing environment strings, and constructing diagnostics. They contain no per-setting field lists.

Do not edit generated files. Do not add a setting directly to `types.ts`, `defaults.ts`, `resolverSections.ts`, `validator.ts`, Rust config structs, `resolver.rs`, or `validation.rs`.

`@octocodeai/config` owns this policy and keeps zero installed runtime dependencies. Ajv is a build/test dependency. The package must not depend on or re-export `@octocodeai/octocode-core`; core owns tool contracts, while config owns configuration and environment policy.

## Precedence and trust tiers

For ordinary settings, highest priority wins:

```text
process environment / MCP client env block
  → trusted project .octocode/.env
  → home .octocode/.env
  → home .octocoderc
  → generated default
```

Project and home `.env` files are propagated only by hosts that use that flow; the native CLI/MCP process normally receives settings through its process environment and `.octocoderc`.

The contract's dotenv policy controls file propagation:

| Policy | Meaning |
|---|---|
| omitted or `all` | May be loaded from a trusted project or home `.env`. |
| `home` | May be loaded from the trusted home `.env`, never a project `.env`. |
| `never` | Shell/CI/MCP environment only; never loaded from either `.env` file. |

## Add a normal setting

For an existing section, adding a normal setting requires one authoritative edit in `config-contract.json` and regeneration.

Example: `output.maxResults`, with `OCTOCODE_MAX_RESULTS`, range 1–500, and default 50:

```json
{
  "sections": {
    "output": {
      "title": "Output",
      "file": true,
      "resolved": true,
      "fields": {
        "maxResults": {
          "type": "number",
          "minimum": 1,
          "span": 499,
          "defaultOffset": 49,
          "description": "Maximum results emitted in one response.",
          "env": {
            "OCTOCODE_MAX_RESULTS": { "priority": 0 }
          }
        }
      }
    }
  }
}
```

Numeric values use `minimum + span` for the maximum and `minimum + min(defaultOffset, span)` for the default. This representation makes inverted ranges impossible by construction. Enum defaults are the first value in `values`; runtime-surface defaults are the first surface.

Then regenerate:

```bash
yarn workspace @octocodeai/config generate:config-contract
```

That one declaration generates:

- `OutputConfigOptions.maxResults?: number`;
- `RequiredOutputConfig.maxResults: number`;
- the resolved default;
- environment precedence and integer parsing;
- clamping and validation bounds;
- unknown-key recognition;
- Rust `OutputConfig.max_results`;
- Rust resolution and validation metadata;
- the settings-reference row and complete example.

Add a focused test proving behavior that is not already guaranteed by the generic interpreter—for example, a downstream feature gate that consumes the new value. Do not add language-parity tests that restate the field declaration manually.

### Supported field shapes

| `type` | Resolved representation | Relevant properties |
|---|---|---|
| `boolean` | boolean / Rust `bool` | `default` |
| `number` | number / Rust `f64` | `minimum`, `span`, `defaultOffset` |
| `string` | string or optional string | `default` |
| `url` | HTTP(S) URL string | `default` |
| `path` | absolute/home path string | `default` |
| `stringArray` | string array or nullable array | `default`, optional `itemFormat: "path"` |
| `enum` | generated literal union / Rust string | `values`, optional `defaultFrom` |
| `schemaVersion` | schema version | reserved for the root version field |

A `null` input means “unset/use the next source.” A `null` generated default becomes an optional string or nullable array where appropriate.

### Environment aliases and invalid input

Bindings are keyed by environment variable and sorted by `priority`; lower numbers win:

```json
"env": {
  "ENABLE_LOCAL": { "priority": 0 },
  "OCTOCODE_ENABLE_LOCAL": { "priority": 1 }
}
```

Optional binding properties:

- `normalize: "trim"` trims a string;
- `normalize: "lower"` trims and lowercases it;
- `invalid: "skip"` ignores an invalid environment value and tries the next source;
- `invalid: "default"` makes an invalid environment value select the generated default rather than file config.

`skip` is the default. Use `default` only when invalid environment input is intentionally authoritative, as with output format.

### Inherited defaults

Use `defaultFrom` rather than copying another default:

```json
"extension.storage.mode": {
  "type": "enum",
  "values": ["persistent", "memory"],
  "defaultFrom": "storage.mode"
}
```

Both generators reject unresolved or cyclic inheritance. At runtime inheritance uses the already-resolved source field, so an environment or file override of `storage.mode` flows into extension storage.

### Add a new section

Declare the section and its fields in the same contract. `file` controls whether it is accepted in `.octocoderc`; `resolved` controls whether it appears in generated `ResolvedConfig` types.

```json
"cache": {
  "title": "Cache",
  "file": true,
  "resolved": true,
  "fields": { }
}
```

Nested sections use dotted names such as `output.pagination`. Parent sections must also be declared, even when their `fields` object is empty.

`typeName` preserves a public TypeScript name when automatic PascalCase is unsuitable. `rustTypeName` does the same for generated Rust structs. These are compatibility metadata, not field policy.

## Add credentials and protected environment keys

Credential values must never appear in `ResolvedConfig`, logs, inspection output, or generated diagnostics. They are read from `effective_env` in Rust or `process.env` in TypeScript consumers.

### Pattern A: environment-only credential

Declare an environment-only policy entry, not a config field:

```json
"environment": {
  "MY_SERVICE_API_KEY": {
    "dotenv": "never",
    "description": "My Service API credential"
  }
}
```

`dotenv: "never"` adds the name to the generated protected-key sets in both languages. The value may come from a shell, CI secret, or MCP client `env` block, but not a home or project `.env`.

Read it without copying it into a loggable structure:

```rust
let key = config.env_value("MY_SERVICE_API_KEY")
    .filter(|value| !value.trim().is_empty());
```

```ts
const key = process.env.MY_SERVICE_API_KEY?.trim() || undefined;
```

If the variable changes resolved configuration source labeling, add `configSource: true`. Authentication-only variables usually should not.

### GitHub token priority

GitHub tokens are Pattern A entries with `tokenPriority`. Lower numbers win. The generator derives `ENV_TOKEN_VARS`, token-source types, and protected-key sets from these declarations:

```json
"OCTOCODE_TOKEN": { "dotenv": "never", "tokenPriority": 0 },
"GH_TOKEN": { "dotenv": "never", "tokenPriority": 1 }
```

Do not add a token array elsewhere.

### Pattern B: environment preferred, trusted `.octocoderc` fallback

Jev uses this pattern. Declare fields in a section with `resolved: false`, mark each field `credential: true`, and give its environment binding `dotenv: "home"` or `never`:

```json
"myService": {
  "title": "My Service",
  "file": true,
  "resolved": false,
  "fields": {
    "key": {
      "type": "string",
      "default": null,
      "credential": true,
      "description": "My Service API key fallback.",
      "env": {
        "MY_SERVICE_API_KEY": {
          "priority": 0,
          "dotenv": "home",
          "normalize": "trim"
        }
      }
    }
  }
}
```

The generic Rust credential adapter applies environment-first file fallback into `effective_env`. The generated input type and generic validators recognize the `.octocoderc` section, but generated resolved types exclude it. `dotenv: "home"` permits the trusted home `.env` and blocks a cloned project's `.env`.

Jev remains the reference: `jev.key`, `jev.model`, and `jev.baseUrl` never enter `ResolvedConfig`.

Tests for Pattern B must prove:

1. process environment wins over `.octocoderc`;
2. `.octocoderc` fills an absent value;
3. trusted home `.env` is accepted when policy is `home`;
4. project `.env` is blocked;
5. `Debug`, inspection JSON, and `get_config_value` do not contain the secret.

## Read resolved configuration

### TypeScript

```ts
import { getConfigSync, getConfigValue } from '@octocodeai/config';

const config = getConfigSync();
const timeout = config.network.timeout;
const format = getConfigValue<'yaml' | 'json'>('output.format');
```

The section-specific `resolveGitHub`, `resolveOutput`, and similar exports remain compatibility adapters. They all delegate to the generic contract interpreter; do not put field logic in them.

### Rust

```rust
let timeout_ms = config.resolved.network.timeout as u64;
let format = &config.resolved.output.format;
let jev_key = config.env_value("OCTOCODE_CLASSIFICATION_API");
```

Rust resolved structs are generated at build time. Native-only builds read and validate both `config-contract.schema.json` and `config-contract.json`; they do not depend on a prior TypeScript generation step or a working-directory-relative path.

## Generated artifacts and checks

Source files:

- `packages/octocode-config/config-contract.json` — authoritative policy;
- `packages/octocode-config/config-contract.schema.json` — contract meta-schema.

Generated files:

- `packages/octocode-config/src/config/contract.generated.ts`;
- `docs/generated/CONFIG_SETTINGS.md`;
- Rust `$OUT_DIR/config_contract.rs` (build output, never commit it).

Commands:

```bash
yarn workspace @octocodeai/config generate:config-contract
yarn workspace @octocodeai/config check:config-contract
yarn workspace @octocodeai/config lint
yarn workspace @octocodeai/config test
yarn workspace @octocodeai/config build

cargo check --manifest-path packages/octocode-native/crates/runtime/Cargo.toml
cargo test --manifest-path packages/octocode-native/crates/runtime/Cargo.toml --lib config::
```

After native/runtime changes, also rebuild the native package and the consuming CLI or MCP interface, then exercise the real CLI path.

## Contributor checklist

### Normal setting

- [ ] Add the field once in `config-contract.json`.
- [ ] Use a contract type and encode default, environment binding, constraint, description, and trust policy there.
- [ ] Regenerate TypeScript and documentation.
- [ ] Add a consumer test for the setting's effect.
- [ ] Run config lint/tests/build and native config tests.
- [ ] Exercise the real CLI/MCP path when runtime behavior changes.

### Credential

- [ ] Choose Pattern A (environment-only) or Pattern B (trusted file fallback).
- [ ] Set `dotenv` deliberately; never rely on an undocumented trust assumption.
- [ ] For Pattern B, set `credential: true` and keep the section `resolved: false`.
- [ ] Verify no secret reaches `ResolvedConfig`, diagnostics, `Debug`, or inspection output.
- [ ] Test process environment, home `.env`, project `.env`, and `.octocoderc` precedence as applicable.

## Common mistakes

| Mistake | Result |
|---|---|
| Editing a generated TypeScript or Rust file | The next generation/build discards the edit. |
| Adding field-specific logic to one resolver | Reintroduces language drift and bypasses contract generation. |
| Copying a default or range into docs | Documentation can drift; generated settings reference owns those facts. |
| Putting a credential in a resolved section | Inspection or point lookup can expose it. |
| Using `dotenv: "all"` for protected infrastructure | A trusted project's `.env` can override the value. |
| Omitting an environment binding from the contract | Source labeling, protection, docs, and both resolvers cannot derive it. |
| Adding config/core coupling | Inverts package ownership; config must remain independent of tool contracts. |
| Testing only compilation | Misses actual CLI/MCP loading, redaction, and interface wiring. |
