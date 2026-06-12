# `octocode-minifier-utils` — Rust Implementation Plan

## Why Rust?

The existing `octocode-minifier` is a pure-TS package that runs inside the MCP server
on every file-read call. The hot paths are:

| TS bottleneck | What it does | Why it's slow |
|---|---|---|
| `removeComments()` | ~400-line char-by-char string scanner | interpreted, GC pressure |
| `extractSignatures()` | 40KB heuristic + TypeScript compiler AST walk | TS compiler startup cost |
| `minifyConservativeCore` | per-line regex passes on every file | regex overhead × line count |
| `minifyJsonCore` | manual parse-and-trim | slower than `serde_json` |
| CleanCSS / html-minifier-terser | external npm libs | each adds own overhead |

Rust equivalents (`lightningcss`, `minify-html`, `oxc`, `serde_json`) are
typically 10–100× faster on those workloads. More importantly, we eliminate
the TypeScript compiler startup cost for signature extraction.

---

## Integration Strategy: `napi-rs` native addon

The package will be a **NAPI-RS** Rust crate that compiles to a `.node` native
addon consumed directly by Node.js — zero subprocess IPC overhead.

```
octocode-minifier-utils/
  Cargo.toml              ← Rust workspace member
  package.json            ← napi-rs JS wrapper (pre-builds for each platform)
  index.ts                ← TypeScript re-export shim (same API surface as octocode-minifier)
  src/
    lib.rs                ← #[napi] exports
    types.rs              ← Strategy, CommentStyle, MinifyResult, FileTypeConfig
    config.rs             ← MINIFY_CONFIG port (100+ extension → strategy mappings)
    file_extension.rs     ← get_extension() — dotfile-aware
    comment_remover.rs    ← string-aware comment removal, 17 comment families
    strategies/
      mod.rs
      conservative.rs     ← strip comments + collapse blank lines, keep indent
      aggressive.rs       ← whitespace compression
      json.rs             ← serde_json parse → re-serialize (minify + readable modes)
      general.rs          ← indent-compressing text minifier
      markdown.rs         ← HTML comments + quote-replies stripped
      css.rs              ← lightningcss
      html.rs             ← minify-html
      js_ts.rs            ← oxc: strip types (TS), minify (JS), terser-equivalent
      python.rs           ← docstring removal + hash comments
    minifier.rs           ← dispatch: minify_content() + minify_content_sync()
    apply.rs              ← apply_minification(), apply_content_view_minification()
    signatures/
      mod.rs
      ts_js.rs            ← oxc AST walk → KeptLine[] skeleton
      heuristic.rs        ← brace/depth line heuristic for all other languages
      renderer.rs         ← NNN| gutter renderer (shared)
    yaml.rs               ← serde_yaml + priority key sorting + block scalar rewrite
  tests/
    comment_remover.rs
    strategies.rs
    signatures.rs
    yaml.rs
```

---

## Rust Crate Mapping

| TS dependency | Rust crate | Notes |
|---|---|---|
| `terser` (JS minify) | `oxc_minifier` | OXC claims 50–100× faster than Babel |
| TypeScript compiler API | `oxc_parser` + `oxc_ast` | Parse to AST, walk declarations |
| CleanCSS | `lightningcss` (Parcel) | 100× faster than CleanCSS in benchmarks |
| `html-minifier-terser` | `minify-html` crate | Pure Rust, actively maintained |
| `js-yaml` | `serde_yaml` | Full YAML 1.2 support |
| String-aware comment parser | custom (port from TS) | Single-pass Rust state machine |
| Brace-depth heuristic | custom (port from TS) | trivially fast in Rust |
| `serde_json` | `serde_json` | Fastest JSON parser in Rust |

---

## API Surface (identical to `octocode-minifier`)

The NAPI exports mirror the existing TS exports 1-for-1 so `octocode-mcp` can
swap the import with zero callers changed.

```typescript
// types
export type Strategy = 'terser' | 'conservative' | 'aggressive' | 'json' | 'general' | 'markdown';
export type MinificationMode = 'content-view' | 'minify' | 'symbols';
export type MinifyResult = { content: string; failed: boolean; type: Strategy | 'failed'; reason?: string };
export type KeptLine = { lineNumber: number; text: string };

// core functions
export function getExtension(filePath: string, options?: GetExtensionOptions): string;
export function minifyContentSync(content: string, filePath: string): string;
export function minifyContent(content: string, filePath: string): Promise<MinifyResult>;
export function applyMinification(content: string, filePath: string): string;
export function applyContentViewMinification(content: string, filePath: string): string;
export function extractSignatures(content: string, filePath: string): KeptLine[] | null;
export function jsonToYamlString(jsonObject: unknown, config?: YamlConversionConfig): string;

// fine-grained strategy exports (used by consumers)
export function removeComments(content: string, style: CommentPatternGroup | CommentPatternGroup[]): string;
export function minifyConservativeCore(content: string, config: FileTypeMinifyConfig): string;
export function minifyAggressiveCore(content: string, config: FileTypeMinifyConfig): string;
export function minifyJsonCore(content: string): MinifyResult;
export function minifyJsonReadable(content: string): MinifyResult;
export function minifyCodeCore(content: string): string;
export function minifyGeneralCore(content: string): string;
export function minifyMarkdownCore(content: string): string;
export function minifyCSSCore(content: string): string;
export function minifyHTMLCore(content: string): string;
export function minifyJavaScriptCore(content: string): string;

// constants
export const MINIFY_CONFIG: MinifyConfig;
export const INDENTATION_SENSITIVE_NAMES: Set<string>;
export const SIGNATURES_ONLY_HINT: string;
```

Async variants (`minifyCSSAsync`, `minifyHTMLAsync`, `minifyWithTerser`, etc.)
become sync in Rust since there is no I/O — the NAPI async wrapper will run
them on the libuv thread pool if callers need the Promise interface.

---

## Implementation Phases

### Phase 1 — Scaffold + infra
- [ ] Create `packages/octocode-minifier-utils/` directory
- [ ] `Cargo.toml`: `[lib] crate-type = ["cdylib"]`, napi-rs deps
- [ ] `package.json`: napi-rs build scripts, platform targets
- [ ] `build.rs`: napi-rs codegen
- [ ] `src/lib.rs`: skeleton `#[napi]` exports
- [ ] Wire into monorepo `Cargo.toml` workspace

### Phase 2 — Types + config
- [ ] Port `MINIFY_CONFIG` (100+ extensions) to `config.rs` as static `HashMap<&str, FileTypeConfig>`
- [ ] Port `INDENTATION_SENSITIVE_NAMES` to a `HashSet<&str>`
- [ ] Port `CommentPatternGroup`, `Strategy`, `MinifyResult`, `KeptLine` types

### Phase 3 — File extension + comment remover
- [ ] `file_extension.rs`: `get_extension()` — handles dotfiles, bare names, full paths
- [ ] `comment_remover.rs`: port the full string-aware state machine:
  - Block comment start/end pairs (17 families)
  - Line comment tokens with boundary checks
  - Quote-delimited string skip (normal quotes, triple-quotes, raw strings)
  - Rust raw strings `r##"..."##`
  - C# verbatim strings `@"..."`
  - PowerShell here-strings `@"...\n"@`
  - Regex literal detection (JS/TS)
  - Shebang preservation (`#!`)
  - Python docstrings (triple-quote detection)

### Phase 4 — Core strategies
- [ ] `strategies/conservative.rs`: comment strip → collapse empty lines, keep indentation
- [ ] `strategies/aggressive.rs`: whitespace normalisation, more compact output
- [ ] `strategies/json.rs`: `minify_json_core` (serde_json parse → serialize), `minify_json_readable`
- [ ] `strategies/general.rs`: indent compression
- [ ] `strategies/markdown.rs`: HTML comment removal, quote-reply stripping
- [ ] `strategies/css.rs`: `lightningcss::StyleSheet::parse → to_css()`
- [ ] `strategies/html.rs`: `minify_html::minify()`
- [ ] `strategies/js_ts.rs`: `oxc_parser::Parser` → print (strip types for TS; compress for JS)
- [ ] `strategies/python.rs`: docstring removal on top of hash-comment stripping

### Phase 5 — Main dispatcher + apply helpers
- [ ] `minifier.rs`: `minify_content_sync` and `minify_content` dispatch tables (mirrors `minifier.ts`)
- [ ] `apply.rs`: `apply_minification`, `apply_content_view_minification`

### Phase 6 — Signature extraction
- [ ] `signatures/ts_js.rs`: OXC AST walk → collect declaration/signature lines:
  - `FunctionDeclaration`, `ArrowFunctionExpression` assigned to `let/const`
  - `ClassDeclaration`, `MethodDefinition`
  - `VariableDeclaration` at module scope
  - TypeScript: `InterfaceDeclaration`, `TypeAliasDeclaration`, `EnumDeclaration`
  - Preserve original 1-based line numbers (`KeptLine`)
- [ ] `signatures/heuristic.rs`: port brace/round/angle depth tracker for 30+ language families:
  - c-family, python, ruby, go, java, rust, kotlin, swift, shell, ...
- [ ] `signatures/renderer.rs`: `render_skeleton()` — `NNN| text` gutter, deduplicate, sort
- [ ] Wire into `extractSignatures()` via extension lookup

### Phase 7 — YAML
- [ ] `yaml.rs`: port `jsonToYamlString()`:
  - `serde_yaml::to_string()` with `forceQuotes` equivalent
  - Priority key sorting (`sortKeys` / `keysPriority` via custom sort fn)
  - Multiline string → block scalar rewrite (`|-` style)

### Phase 8 — NAPI exports + TS shim
- [ ] Expose all Phase 2–7 functions as `#[napi]` in `src/lib.rs`
- [ ] Write `index.ts` shim that re-exports from `.node` with proper TS types
- [ ] Add `index.d.ts` generated by napi-rs

### Phase 9 — Tests (90% coverage parity)
- [ ] Port all `octocode-minifier/tests/*.test.ts` → Rust `#[cfg(test)]` modules
- [ ] Add roundtrip tests: feed TS file → get same skeleton as the TS impl
- [ ] Fuzz comment removal with arbitrary content
- [ ] Benchmark vs TS impl with `criterion`

### Phase 10 — Integration + migration
- [ ] `octocode-mcp` switches import from `@octocodeai/octocode-minifier` to `@octocodeai/octocode-minifier-utils`
- [ ] CI: add Rust build + test steps
- [ ] Publish pre-built binaries for: `darwin-arm64`, `darwin-x64`, `linux-arm64`, `linux-x64`, `linux-x64-musl`, `windows-x64`
- [ ] Keep TS package as fallback for unsupported platforms (graceful degradation)

---

## Key Design Decisions

### No `unsafe` in hot paths
All comment removal and minification logic is safe Rust. `unsafe` only where
napi-rs internals require it (behind the `#[napi]` macro).

### Sync-first; async at the NAPI boundary
Every Rust function is synchronous. The NAPI layer wraps long-running operations
(CSS/HTML/JS minification) in `AsyncTask` to avoid blocking the Node.js event loop.
This matches the existing `minifyContent` (async) vs `minifyContentSync` split.

### Graceful fallback
`minify_content` and `apply_content_view_minification` catch panics (`std::panic::catch_unwind`)
and return the original content rather than crashing the MCP server — same contract
as the TS `try/catch` guards.

### 1MB size guard
Port the `MAX_SIZE = 1024 * 1024` guard before any heavy processing.

### Exact API parity
The `octocode-mcp` package currently imports:
```ts
import { applyContentViewMinification, extractSignatures, jsonToYamlString } from '@octocodeai/octocode-minifier';
```
The Rust package exports identical symbols — drop-in replacement.

---

## Expected Performance Gains

| Operation | TS (est.) | Rust (est.) | Gain |
|---|---|---|---|
| `removeComments` on 50KB file | ~5ms | ~0.1ms | ~50× |
| `minifyJsonCore` on 100KB JSON | ~8ms | ~0.5ms | ~16× |
| `minifyCSSCore` on 20KB CSS | ~15ms (CleanCSS) | ~0.3ms (lightningcss) | ~50× |
| `minifyHTMLCore` on 30KB HTML | ~12ms | ~0.5ms | ~24× |
| `minifyWithTerser` on 10KB JS | ~25ms | ~2ms (oxc) | ~12× |
| `extractSignatures` on 40KB TS | ~80ms (TS compiler) | ~3ms (oxc) | ~27× |
| `applyContentViewMinification` | ~10ms avg | ~0.3ms avg | ~33× |

These translate directly to lower latency on `localGetFileContent` and
`githubGetFileContent` calls in the MCP server.

---

## File Count Summary

| Module | Rust files | Complexity |
|---|---|---|
| types + config | 2 | medium (100+ entries) |
| file_extension | 1 | trivial |
| comment_remover | 1 | hard (state machine, 17 families) |
| strategies | 9 | medium–hard |
| minifier + apply | 2 | medium |
| signatures | 3 | hard (OXC AST + heuristic) |
| yaml | 1 | medium |
| lib.rs | 1 | thin glue |
| **Total** | **~20 files** | |

---

## Open Questions / Risks

| Risk | Mitigation |
|---|---|
| OXC signature extraction fidelity vs TypeScript compiler | Port tests from TS impl, run both and diff output |
| `serde_yaml` block scalar output differs from `js-yaml` | Property-test against known YAML snapshots |
| Platform support gaps (Alpine musl, old glibc) | Provide `@octocode-minifier` TS fallback in `package.json#optionalDependencies` |
| napi-rs version compatibility with Node 20 | Pin napi 2.x; test in CI on Node 20 + 22 |
| OXC API stability | OXC is 1.x stable; pin minor version |
