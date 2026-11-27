Load when adding, renaming or retiring a setting.

# Adding configuration to Octocode

How to add a setting or credential without TypeScript/Rust drift. User-facing settings: `<repo>/docs/CONFIGURATION.md`; credential flows: `<repo>/docs/AUTHENTICATION.md`. Paths below are under `packages/octocode-config/` unless they start with `crates/` (`packages/octocode-native/crates/`).

- Source precedence, environment bindings and dotenv policy: [CONFIG_ENVIRONMENT.md](CONFIG_ENVIRONMENT.md)
- Credentials, tokens, and keys a skill or script reads: [CONFIG_CREDENTIALS.md](CONFIG_CREDENTIALS.md)

## Architecture

The Zod declaration in `scripts/config-contract/declaration/` (`sections.ts`, `environment.ts`), shaped by `scripts/config-contract/schema.ts`, is the only declaration of field policy: paths, types, defaults, one environment variable per setting, enums, URL and path semantics, unknown keys, dotenv trust, credential exclusion, descriptions. The generator validates it and emits `config-contract.json` and `config-contract.schema.json`. From that JSON:

- the TypeScript generator writes `src/config/contract.generated.ts` and the settings table between the markers in `<repo>/docs/CONFIGURATION.md`;
- `crates/runtime-base/build.rs` writes `$OUT_DIR/config_contract.rs` (structs, defaults, field metadata, env policy) by manifest-relative path, so a native-only build needs no TypeScript step; its renderer fails closed on missing or mistyped fields. The generic Rust resolver and validator are the only resolver.

Never edit generated files, add a setting to the Rust config structs, `resolver.rs` or `validation.rs`, or copy a default or range into docs. Set `displayDefault` on a setting whose `null` default means something. TypeScript surfaces never resolve settings: the native runtime does, and `octocode config` shows the result. The package's `.` entry uses only Node builtins; `src/home.ts`, `src/dotenv.ts` and `src/config` never import core (the `./schema` and `./mcp` subpaths re-export it).

## What is a setting

Something a user chooses: a credential, endpoint, feature switch, tool list, scope, storage mode, model or presentation format. Internal limits (timeouts, retries, page sizes, TTLs, concurrency, spend and memory caps) are code constants, never settings; the contract has no number type. A shared limit lives in `RuntimeLimits::DEFAULT` (`crates/runtime-base/src/execution/limits.rs`; tests vary it through `ToolRuntime::with_limits`), any other next to its one use with a doc comment (for example `IDLE_EXIT` in `crates/cli/src/cli/serve.rs`). A limit that bounds evidence is disclosed in the response. Docs link to the constant instead of copying its value.

To retire a setting, delete its declaration, consumers and tests, then regenerate; unknown names get ordinary unknown-key handling, no tombstones. To rename one, rename it everywhere and regenerate; the old name is an unknown key, with no alias or legacy mechanism.

## Add a setting

One edit in `declaration/sections.ts`, then `yarn workspace @octocodeai/config generate:config-contract`. Hypothetical `output.compact`: in the `output` section's `fields`, add `"compact": { "type": "boolean", "default": false, "description": "Render compact text content.", "env": { "name": "OCTOCODE_OUTPUT_COMPACT" } }`. Generation produces the TypeScript type and default, env parsing and precedence, validation, unknown-key recognition, the Rust field, and the rows in `config list` and the settings table. Test only an effect the generic interpreter does not guarantee (a downstream feature gate), never parity that restates the declaration.

- Field types: `boolean`, `string`, `url` (HTTP(S)), `path` (absolute or home) take `default`; `stringArray` takes `default` and optional `itemFormat: "path"`; `enum` takes `values` (the first is the default) and optional `typeName`. A `null` input means unset; a `null` default makes an optional string or nullable array.
- A section: `"cache": { "title": "Cache", "fields": { } }` in `sections.ts`. Every section belongs to `.octocoderc` and `ResolvedConfig`, credential fields apart. Nested sections use dotted names (`parent.child`); declare the parents too.
- Native code reads `config.resolved.<section>.<field>` (for example `config.resolved.tools.enable_clone`) and `config.env_value("OCTOCODE_TYPESAFE_KEY")`.

## Checks

```bash
yarn workspace @octocodeai/config generate:config-contract
yarn workspace @octocodeai/config test
node skills-dev/octocode-dev/scripts/dev.mjs test:dev --crate runtime-base -- config::
```

A compile-only check misses CLI and MCP loading, redaction and interface wiring: exercise `octocode config --json` and one MCP session per setting.
