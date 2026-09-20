# Adding config to Octocode — developer guide

This guide is for contributors who need to add a new setting or credential to Octocode. It traces the complete flow across both layers of the config stack so you make every touch point in one pass and never end up with a setting that is parsed in TypeScript but silently ignored in Rust (or vice versa).

---

## Table of contents

- [Architecture in one diagram](#architecture-in-one-diagram)
- [Two categories, three file roles](#two-categories-three-file-roles)
- [Adding a regular behavioral setting](#adding-a-regular-behavioral-setting)
  - [1 — TypeScript: `types.ts`](#1--typescript-typests)
  - [2 — TypeScript: `defaults.ts`](#2--typescript-defaultsts)
  - [3 — TypeScript: `resolverSections.ts`](#3--typescript-resolversectionsts)
  - [4 — TypeScript: `validator.ts`](#4--typescript-validatorts)
  - [5 — Rust: `types.rs`](#5--rust-typesrs)
  - [6 — Rust: `resolver.rs` — `SOURCE_KEYS` + `resolve_sections`](#6--rust-resolverrs--source_keys--resolve_sections)
  - [7 — Rust: `validation.rs`](#7--rust-validationrs)
  - [8 — Documentation: `CONFIGURATION.md`](#8--documentation-configurationmd)
- [Adding a credential / protected key](#adding-a-credential--protected-key)
  - [Pattern A — env-only (GitHub-token style)](#pattern-a--env-only-github-token-style)
  - [Pattern B — env preferred, `.octocoderc` fallback (Jev style)](#pattern-b--env-preferred-octocoderc-fallback-jev-style)
- [Reading config in tools and implementation](#reading-config-in-tools-and-implementation)
  - [TypeScript consumers](#typescript-consumers)
  - [Rust consumers](#rust-consumers)
- [End-to-end checklist](#end-to-end-checklist)
- [Common mistakes](#common-mistakes)

---

## Architecture in one diagram

```
User writes                    Loaded by           Written into
────────────────────────────── ─────────────────── ───────────────────────────────

Shell / CI env vars  ─────────┐
MCP client env block ─────────┤                    effective_env  (Rust BTreeMap)
                              │  Rust acquire +    ──────────────────────────────
~/.octocode/.env  ────────────┤  resolve_config    ResolvedConfig struct  (Rust)
~/.octocode/.octocoderc ──────┘  │                 ──────────────────────────────
                                 │  TS wrapper      getConfigSync()  (TypeScript)
<project>/.octocode/.env ────────┘
(trusted projects only)
```

Priority (highest → lowest):

```
1. process env (shell export / MCP env block)
2. <project>/.octocode/.env     ← skills/agents only, never MCP server / CLI
3. ~/.octocode/.env             ← skills/agents only, never MCP server / CLI
4. ~/.octocode/.octocoderc      ← MCP server + CLI, JSONC
5. built-in defaults
```

The Rust runtime and the TypeScript wrapper are **parallel implementations of the same resolution rules**. Both must be updated together; the Rust layer is what actually runs; the TypeScript layer is used by the CLI and Pi extension to read config from JavaScript.

---

## Two categories, three file roles

| Category | Where to put it | Read by |
|----------|----------------|---------|
| Behavioral setting (timeout, output format, tool gate…) | env var **or** `~/.octocode/.octocoderc` | MCP server, CLI, Pi extension |
| Third-party API key for skills (Tavily, Serper…) | `~/.octocode/.env` | Agent sessions and skills only |
| Credential / protected key (GitHub token, Jev key) | env var only (or `.octocoderc` section for Jev-style) | See [Adding a credential](#adding-a-credential--protected-key) |

Never put credentials in `.env` unless you follow Pattern B exactly (which uses a special `.octocoderc` section and explicitly blocks the project `.env`).

---

## Adding a regular behavioral setting

Use this section when you are adding a toggle, a limit, a path, or any non-secret config option. The example throughout is a hypothetical `output.maxResults` setting backed by `OCTOCODE_MAX_RESULTS`.

### 1 — TypeScript: `types.ts`

File: `packages/octocode-config/src/config/types.ts`

Add the optional field to the relevant `*ConfigOptions` interface and the required field to the matching `Required*Config` interface.

```ts
// In OctocodeConfig sub-interface
export interface OutputConfigOptions {
  format?: 'yaml' | 'json';
  pagination?: OutputPaginationConfigOptions;
  maxResults?: number;      // ← add here
}

// In the fully-resolved Required* interface
export interface RequiredOutputConfig {
  format: 'yaml' | 'json';
  pagination: RequiredOutputPaginationConfig;
  maxResults: number;       // ← add here (required, never undefined after resolution)
}
```

If your setting lives in a new top-level section, add a new `XxxConfigOptions` interface **and** a new `RequiredXxxConfig` interface, then add both to `OctocodeConfig` and `ResolvedConfig`.

---

### 2 — TypeScript: `defaults.ts`

File: `packages/octocode-config/src/config/defaults.ts`

Add the default value to the matching `DEFAULT_*` constant. Export any bounds constants if the value is range-clamped.

```ts
export const MAX_RESULTS_MIN = 1;
export const MAX_RESULTS_MAX = 500;

export const DEFAULT_OUTPUT_CONFIG: RequiredOutputConfig = {
  format: 'yaml',
  pagination: { defaultCharLength: 20000 },
  maxResults: 50,   // ← add here
};
```

---

### 3 — TypeScript: `resolverSections.ts`

File: `packages/octocode-config/src/config/resolverSections.ts`

Wire the env var → file config → default precedence chain inside the matching `resolve*` function. Use the helpers already in the file (`parseBooleanEnv`, `parseIntEnv`, `parseStringArrayEnv`).

```ts
export function resolveOutput(
  fileConfig?: OctocodeConfig['output']
): RequiredOutputConfig {
  // … existing resolution …

  const envMaxResults = parseIntEnv(process.env.OCTOCODE_MAX_RESULTS);
  const configuredMaxResults =
    envMaxResults ??
    fileConfig?.maxResults ??
    DEFAULT_OUTPUT_CONFIG.maxResults;
  const clampedMaxResults = Math.max(
    MAX_RESULTS_MIN,
    Math.min(MAX_RESULTS_MAX, configuredMaxResults)
  );

  return {
    format: /* … */,
    pagination: { defaultCharLength: /* … */ },
    maxResults: clampedMaxResults,   // ← add here
  };
}
```

For a new top-level section, write a new `resolveXxx(fileConfig?)` function following the same pattern, add it to the exports at the top of `index.ts`, and call it in the TypeScript `ResolvedConfig` construction path (usually in `resolverCache.ts` which delegates to these resolver functions).

---

### 4 — TypeScript: `validator.ts`

File: `packages/octocode-config/src/config/validator.ts`

Two places to update:

**a) Validation function** — add type/range checks inside the relevant `validate*` function:

```ts
function validateOutput(output: unknown, errors: string[]): void {
  // … existing checks …
  if (out.maxResults !== undefined) {
    const err = validateNumberRange(
      out.maxResults, 'output.maxResults',
      MAX_RESULTS_MIN, MAX_RESULTS_MAX
    );
    if (err) errors.push(err);
  }
}
```

**b) Unknown-key warning** — add the new field to the `warnUnknownObjectKeys` call for your section:

```ts
warnUnknownObjectKeys(
  cfg.output,
  'output',
  ['format', 'pagination', 'maxResults'],   // ← add here
  warnings
);
```

Missing either step means misspellings are silently ignored instead of surfacing as warnings.

---

### 5 — Rust: `types.rs`

File: `packages/octocode-native/crates/runtime/src/config/types.rs`

Add the field to the matching Rust struct. Match the camelCase renaming that serde uses, since the JSON round-trip uses the same key names as the TypeScript side.

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub format: String,
    pub pagination: PaginationConfig,
    pub redact_emails: bool,
    #[serde(rename = "maxResults", default)]
    pub max_results: f64,   // ← add here
}
```

For a new top-level section, define a new struct and add it as a field on `ResolvedConfig`.

---

### 6 — Rust: `resolver.rs` — `SOURCE_KEYS` + `resolve_sections`

File: `packages/octocode-native/crates/runtime/src/config/resolver.rs`

**a) Add the env var name to `SOURCE_KEYS`** so the config source label (`Env`, `Mixed`, etc.) is computed correctly:

```rust
const SOURCE_KEYS: [&str; 25] = [   // bump count
    // … existing …
    "OCTOCODE_MAX_RESULTS",          // ← add here
];
```

**b) Wire the resolution inside `resolve_sections`**, following the same env → file → default pattern as TypeScript:

```rust
output: OutputConfig {
    format: { /* … */ },
    pagination: PaginationConfig { /* … */ },
    redact_emails: /* … */,
    max_results: {
        let raw = parse_int_env(env(e, "OCTOCODE_MAX_RESULTS"))
            .map(|x| x as f64)
            .or_else(|| num_field(output, "maxResults"))
            .unwrap_or(50.);
        clamp(raw, 1., 500.)
    },
},
```

---

### 7 — Rust: `validation.rs`

File: `packages/octocode-native/crates/runtime/src/config/validation.rs`

Mirror the TypeScript validator: add a range check and add the field to the known-keys set for the unknown-key warning.

```rust
// Inside validate_output():
if let Some(v) = out.get("maxResults") {
    validate_number_range(v, "output.maxResults", 1.0, 500.0, errors);
}

// In the warnUnknownObjectKeys equivalent:
let known_output = ["format", "pagination", "redactEmails", "maxResults"];
```

---

### 8 — Documentation: `CONFIGURATION.md`

File: `docs/CONFIGURATION.md`

Add a row to the relevant table in [All settings reference](CONFIGURATION.md#all-settings-reference). Include the env var name, the `.octocoderc` key path, the default, and any range or note.

```md
| `OCTOCODE_MAX_RESULTS` | `output.maxResults` | `50` | 1 – 500 |
```

Also add the field to the annotated `.octocoderc` reference block in that doc so users can copy-paste a working example.

---

## Adding a credential / protected key

Credentials need special handling. Never put a secret in `ResolvedConfig` (it can be dumped via `config get`) and never let it come from an untrusted project `.env`.

### Pattern A — env-only (GitHub-token style)

Use this pattern when the credential must **only** come from the process environment — never from any file.

**Step 1 — TypeScript `PROTECTED_KEYS`**

`packages/octocode-config/src/index.ts`

```ts
export const PROTECTED_KEYS: ReadonlySet<string> = new Set([
  // … existing …
  'MY_SERVICE_API_KEY',   // ← add here
]);
```

**Step 2 — Rust `PROTECTED_KEYS`**

`packages/octocode-native/crates/runtime/src/config/types.rs`

```rust
pub const PROTECTED_KEYS: [&str; 20] = [   // bump count
    // … existing …
    "MY_SERVICE_API_KEY",   // ← add here
];
```

**Step 3 — Read in tools via `effective_env` / `env_value`**

```rust
// In a tool handler
let key = config_output.env_value("MY_SERVICE_API_KEY")
    .filter(|s| !s.trim().is_empty());
```

Do **not** put the key in `ResolvedConfig` or any loggable struct field.

**Step 4 — Document it**

Add a row in the "Protected keys" table in `CONFIGURATION.md` with the reason it is protected. Add a row in the "Third-party keys" section if it is meant to be set by users via their shell.

---

### Pattern B — env preferred, `.octocoderc` fallback (Jev style)

Use this pattern when you want to let users store a credential in `.octocoderc` as a convenience (same trust tier as `github.apiUrl`), while still blocking it from the project `.env`.

The Jev key is the canonical example. Study `apply_jev_file_fallback` in `resolver.rs`.

**Step 1 — Add a section to `OctocodeConfig` (TypeScript)**

```ts
// types.ts
export interface MyServiceConfigOptions {
  key?: string | null;
  baseUrl?: string | null;
}

export interface OctocodeConfig {
  // … existing …
  myService?: MyServiceConfigOptions;
}
```

The section is read from `.octocoderc` only — it must **not** appear in `ResolvedConfig` (to prevent `config get` from leaking the value).

**Step 2 — Validate the new section (TypeScript)**

`validator.ts` — add a `validateMyService` function and call it from `validateConfig`. Add the section name to the top-level known-keys set. Add `warnUnknownObjectKeys` for the new section's fields.

**Step 3 — Apply the file fallback after env is resolved (Rust)**

`resolver.rs` — add a function mirroring `apply_jev_file_fallback`:

```rust
fn apply_my_service_file_fallback(file: Option<&Value>, effective: &mut BTreeMap<String, String>) {
    let section = object(file, "myService");
    for (env_key, file_key) in [
        ("MY_SERVICE_API_KEY", "key"),
        ("MY_SERVICE_BASE_URL", "baseUrl"),
    ] {
        if effective.get(env_key).is_some_and(|v| !v.trim().is_empty()) {
            continue;   // env wins
        }
        if let Some(v) = str_field(section, file_key).filter(|s| !s.trim().is_empty()) {
            effective.insert(env_key.to_owned(), v);
        }
    }
}
```

Call it in `resolve_config`, just like `apply_jev_file_fallback`:

```rust
apply_jev_file_fallback(file.as_ref(), &mut effective);
apply_my_service_file_fallback(file.as_ref(), &mut effective);  // ← add
```

**Step 4 — Add to `PROTECTED_KEYS` in both layers**

The env var names (`MY_SERVICE_API_KEY`, `MY_SERVICE_BASE_URL`) must be in `PROTECTED_KEYS` in both TypeScript (`index.ts`) and Rust (`types.rs`) so a malicious project `.env` cannot set them. The `.octocoderc` read is safe because it only reads from `octocode_home` (user-controlled), not from a cloned project.

**Step 5 — Add to `SOURCE_KEYS` in Rust**

So the config source label (`Env` vs `Mixed` vs `File`) is computed correctly.

**Step 6 — Never put the value in `ResolvedConfig`**

The credential lives in `effective_env` / `child_env` only. Tools read it via `config_output.env_value("MY_SERVICE_API_KEY")`. The CLI `config get myService.key` must return nothing — verify with a test.

**Step 7 — Document under "Protected keys"**

Explain the protection rule and the `.octocoderc` convenience fallback. See the Jev section in `CONFIGURATION.md` as the template.

---

## Reading config in tools and implementation

### TypeScript consumers

```ts
import { getConfigSync, getConfigValue } from '@octocodeai/config';

// Full resolved config
const config = getConfigSync();
const timeout = config.network.timeout;

// Point lookup (returns undefined if path is invalid)
const fmt = getConfigValue<'yaml' | 'json'>('output.format');

// For credentials that live in effective_env only, read process.env directly
// (they are propagated there by propagateOctocodeEnv before the process starts)
const jevKey = process.env.OCTOCODE_JEV_KEY;
```

The TypeScript resolver caches the result per process; there is no need to call `getConfigSync()` on every request.

### Rust consumers

Inside a tool handler or runtime component the config is already resolved and passed as `ConfigOutput`:

```rust
// Behavioral settings — read from resolved struct
let timeout_ms = config.resolved.network.timeout as u64;
let fmt = &config.resolved.output.format;

// Credentials — read from effective_env; never from resolved struct
let jev_key = config.env_value("OCTOCODE_JEV_KEY");
let my_key  = config.env_value("MY_SERVICE_API_KEY");
```

`env_value` returns `Option<&str>`. Always filter for blank:

```rust
let key = config.env_value("MY_SERVICE_API_KEY")
    .filter(|s| !s.trim().is_empty());
```

---

## End-to-end checklist

### Regular behavioral setting

- [ ] `packages/octocode-config/src/config/types.ts` — add to `*ConfigOptions` and `Required*Config`
- [ ] `packages/octocode-config/src/config/defaults.ts` — add to `DEFAULT_*` constant; export bounds if clamped
- [ ] `packages/octocode-config/src/config/resolverSections.ts` — wire env var → file → default in the matching `resolve*` function
- [ ] `packages/octocode-config/src/config/validator.ts` — validate type/range + add field to `warnUnknownObjectKeys` call
- [ ] `packages/octocode-native/crates/runtime/src/config/types.rs` — add field to the matching Rust struct
- [ ] `packages/octocode-native/crates/runtime/src/config/resolver.rs` — add env var to `SOURCE_KEYS` + wire resolution in `resolve_sections`
- [ ] `packages/octocode-native/crates/runtime/src/config/validation.rs` — mirror the TypeScript validator
- [ ] `docs/CONFIGURATION.md` — add row to the reference table + annotated `.octocoderc` block
- [ ] Tests — add a test to `packages/octocode-config/src/config/mod.rs` (TypeScript) and to the Rust `config/mod.rs` inline tests

### Credential / protected key (Pattern A — env-only)

- [ ] `packages/octocode-config/src/index.ts` — add to `PROTECTED_KEYS`
- [ ] `packages/octocode-native/crates/runtime/src/config/types.rs` — add to `PROTECTED_KEYS`
- [ ] Read via `config_output.env_value()` (Rust) or `process.env` (TypeScript); never store in `ResolvedConfig`
- [ ] `docs/CONFIGURATION.md` — add to "Protected keys" table with reason

### Credential / protected key (Pattern B — `.octocoderc` fallback, Jev style)

- [ ] `packages/octocode-config/src/config/types.ts` — add `*ConfigOptions` section to `OctocodeConfig` only (not to `ResolvedConfig`)
- [ ] `packages/octocode-config/src/config/validator.ts` — validate the new section + `warnUnknownObjectKeys`
- [ ] `packages/octocode-native/crates/runtime/src/config/resolver.rs` — write `apply_xxx_file_fallback` + call it in `resolve_config`
- [ ] Both `PROTECTED_KEYS` arrays (TypeScript + Rust) — add env var names
- [ ] `SOURCE_KEYS` in Rust `resolver.rs` — add env var names
- [ ] Verify `get_config_value(resolved, "myService.key")` returns `None` (never leaks)
- [ ] `docs/CONFIGURATION.md` — document under the relevant sub-section and "Protected keys" table
- [ ] Tests — cover file fallback, env wins over file, project `.env` is blocked, `Debug` output does not contain the secret value

---

## Common mistakes

| Mistake | Symptom | Fix |
|---------|---------|-----|
| Updated TypeScript resolver but not Rust `resolve_sections` | Setting works in Pi extension but is silently ignored in MCP/CLI at runtime | Update both layers; the Rust runtime is what executes |
| Added to `types.ts` but forgot `validator.ts` `warnUnknownObjectKeys` | Misspelled key in `.octocoderc` silently falls back to default | Add the field name to the relevant `warnUnknownObjectKeys` call |
| Added env var but forgot `SOURCE_KEYS` in Rust | Config source label stays `Defaults` even when the env var is set; confusing `config --json` output | Add the env var string to the `SOURCE_KEYS` array |
| Put a credential in `ResolvedConfig` | `octocode config --json` or `get_config_value` can dump the secret | Keep credentials in `effective_env` only; read via `env_value()` |
| Put a credential in `.env` or in `OctocodeConfig` without adding to `PROTECTED_KEYS` | A cloned project `.env` can override it | Add to both `PROTECTED_KEYS` arrays |
| Forgot to add to TypeScript `index.ts` exports | Consumers outside the package cannot import the new type or function | Export from `src/index.ts` |
| Set a default only in TypeScript, not in Rust | Default differs between surfaces; `config get` returns a different value than the MCP server uses | Set the default in both `defaults.ts` and the matching Rust `unwrap_or` call in `resolve_sections` |
