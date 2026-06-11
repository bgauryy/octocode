# `@octocodeai/octocode-minifier`

Token-efficient compression for AI agent context windows. Reduces file content, PR patches, and search results before they enter the LLM — while keeping the output navigable by an agent, not just byte-smaller.

> **Not a production build optimizer.** Use [Terser](https://www.npmjs.com/package/terser), [esbuild](https://www.npmjs.com/package/esbuild), or [Lightning CSS](https://www.npmjs.com/package/lightningcss) for deployable output. This package optimizes for agent-readable token reduction.

---

## Contents

- [Install & build](#install--build)
- [Quick start](#quick-start)
- [API reference](#api-reference)
- [Three modes](#three-modes)
- [Format support](#format-support)
- [Benchmarks](#benchmarks)
- [Architecture](#architecture)
- [Development](#development)
- [Known limitations](#known-limitations)

---

## Install & build

```bash
yarn install
yarn build          # lint + typecheck + emit dist/
yarn test           # vitest run + v8 coverage
```

Requirements: Node ≥ 20, TypeScript ≥ 5.

---

## Quick start

```ts
import {
  applyContentViewMinification,  // safe default for file reading tools
  applyMinification,             // sync, keeps original on failure/growth
  minifyContent,                 // async, best available engine
  minifyContentSync,             // sync raw strategy output
  extractSignatures,             // navigation skeleton with line numbers
  jsonToYamlString,              // YAML serialisation for MCP responses
} from '@octocodeai/octocode-minifier';

// Readable compressed output — safe default, never grows content
const readable = applyContentViewMinification(source, 'src/app.ts');

// Strongest available compression for the format
const compact = await minifyContent(source, 'src/app.ts');
// → { content, failed, type: 'conservative' | 'terser' | ..., reason? }

// Navigation skeleton with original line numbers
const skeleton = extractSignatures(source, 'src/app.ts');
// → " 12| export function search(query: Query): Promise<Result> {\n 31| class Index {"

// Structured MCP response serialisation
const yaml = jsonToYamlString({ tool: 'read_file', result: data });
```

---

## API reference

### `applyContentViewMinification(content, filePath) → string`

**Use for:** MCP file-content tools, `localGetFileContent`, `githubGetFileContent`, PR patch context.

Applies comment stripping and whitespace compression appropriate for the file type. Preserves indentation and line structure so agents can read the output. Returns the original if the result is not shorter.

```ts
const out = applyContentViewMinification(src, 'server.go');
// Comments stripped, blank lines compressed, indentation preserved.
```

### `applyMinification(content, filePath) → string`

**Use for:** Synchronous snippets where you want the most compact safe output.

Calls `minifyContentSync` and returns the original if output grows. Never throws.

### `minifyContentSync(content, filePath) → string`

Raw synchronous strategy dispatch. Returns the strategy output even if it is longer (no guard). Prefer `applyMinification` unless you need the raw output.

### `minifyContent(content, filePath) → Promise<MinifyResult>`

**Use for:** Any path where async is allowed and maximum compression matters.

Dispatches to the strongest available engine for the format (Terser for JS/JSX, CleanCSS for CSS, html-minifier-terser for HTML, TypeScript compiler + Terser for TS). Returns:

```ts
type MinifyResult = {
  content: string;
  failed: boolean;
  type: 'terser' | 'conservative' | 'aggressive' | 'json' | 'markdown' | 'general' | 'failed';
  reason?: string; // set on failure or degraded path
};
```

Files larger than 1 MB are returned unchanged with `failed: true`.

### `extractSignatures(content, filePath) → string | null`

**Use for:** "Show me the structure first" navigation workflows.

Returns a whole-file skeleton with the original 1-based line numbers in the left gutter. Bodies and comments are dropped. Returns `null` when the extension is unsupported or extraction produces no results.

```txt
 1| import { serve } from 'bun';
 8| export interface Config {
12|   host: string;
14| }
17| export function createServer(config: Config): Server {
31| export class Router {
```

Supported extensions: `bash c cc cjs cpp cs css go h hpp html htm java js jsx kotlin kt less mjs php py rb rs rust scala scss sh sql svelte swift ts tsx vue zsh`

### `jsonToYamlString(obj, config?) → string`

Serialises any JSON-compatible value to YAML using `js-yaml`. Multi-line string values are automatically converted to block scalars (`|-`) for readability. Accepts optional `sortKeys` and `keysPriority` for stable output ordering.

```ts
const yaml = jsonToYamlString({ name: 'read_file', path: 'src/app.ts' });
```

---

## Three modes

| Mode | Function | Purpose | Output contract |
|---|---|---|---|
| **content-view** | `applyContentViewMinification` | Readable agent context — comment removal, whitespace compression, indentation preserved | Never grows. Not executable. |
| **minify** | `minifyContent` / `minifyContentSync` | Maximum supported compression | Parser-backed for JS/TS/CSS/HTML; strategy-backed otherwise. |
| **symbols** | `extractSignatures` | Navigation skeleton only | Lossy. Use `startLine`/`endLine` to fetch bodies. |

---

## Format support

**138 registered extensions.** Every registered extension gets comment stripping appropriate for the language and whitespace normalisation.

### Parser / engine backed

Strongest compression path. Uses production parsing libraries.

| Extensions | Strategy | Engine | Symbols |
|---|---|---|---|
| `js` `mjs` `cjs` | terser | Terser compress + format | ✓ |
| `jsx` | terser | TypeScript transpile → Terser | ✓ |
| `ts` `tsx` | conservative + Terser candidate | TypeScript compiler → Terser when shorter | ✓ |
| `css` `less` `scss` | aggressive | CleanCSS (async) · regex (sync) | ✓ |
| `html` `htm` | aggressive | html-minifier-terser (async) · regex (sync) | ✓ |
| `vue` `svelte` | aggressive | HTML + embedded script/style minification | ✓ |
| `json` `jsonc` `json5` | json | JSON parse → compact stringify; JSONC/JSON5 stripped first | — |

### Conservative (comment stripping + whitespace)

Preserves line structure and indentation — agents can still navigate.

```
py go java c h cpp hpp cc cs rs swift kt kotlin scala dart
rb php perl sh bash zsh fish ps1 psm1 psd1
sql tsql plsql graphql gql proto
yaml yml toml ini conf config env properties csv
haml slim sass styl coffee nim jl v zig nix
tf tfvars pp bzl cmake star awk
fs fsx hs lhs elm lisp lsp scm rkt clj cljs
vhd vhdl adb ads f for f90 f95 f03 f08
asm nasm wat wast rst
cfg gitignore dockerignore
```

### Aggressive (stronger text cleanup)

```
lua r hbs handlebars ejs mustache twig jinja jinja2 erb
pl pm erl hrl clj cljs
```

### Markdown and text

| Extensions | Behaviour |
|---|---|
| `md` `markdown` | Strips HTML comments, quote-reply blocks (`> …`), trailing whitespace, excessive blank lines |
| `txt` `log` | Whitespace-only normalisation |
| unknown extension | General whitespace fallback |

### Extensionless filenames

Routed through indentation-preserving conservative cleanup:

```
Makefile  Dockerfile  Procfile  Justfile  Rakefile  Gemfile
Podfile   Fastfile    Vagrantfile  Jenkinsfile  Cakefile
Pipfile   Buildfile   Capfile  Brewfile
```

---

## Benchmarks

Measured on **two corpora:**

1. **Real-corpus** — actual open-source files per extension, measured against 45 languages.
2. **Large-file** — authentic ~400-line samples per language, validated by tests in `tests/large-file-benchmark.test.ts`.

### Large-file benchmark (12 languages, ~400 lines each)

> Source samples: httpx, Spring StringUtils, nlohmann/json, tokio runtime, Lodash-style utils, Kotlin repository pattern, Bootstrap-style design system, GitHub Actions CI/CD, PostgreSQL e-commerce schema, nvm-style deploy script, ActiveRecord model.

| Ext | Language | Lines | Content-view | Apply | Async | Symbols | Rating |
|---|---|---:|---:|---:|---:|---:|---|
| `.js` | JavaScript | 419 | −54.1% | −61.9% | −61.9% | −91.3% | **10/10 excellent** |
| `.ts` | TypeScript | 322 | −25.9% | −61.4% | −61.4% | −71.3% | **9.5/10 excellent** |
| `.java` | Java | 394 | −54.7% | −54.7% | −54.7% | −83.3% | **9/10 excellent** |
| `.rb` | Ruby | 201 | −55.4% | −55.4% | −55.4% | −92.8% | **9/10 excellent** |
| `.go` | Go | 323 | −34.5% | −34.5% | −34.5% | −77.7% | **7.5/10 good** |
| `.rs` | Rust | 325 | −33.2% | −33.2% | −33.2% | −80.8% | **7.5/10 good** |
| `.kt` | Kotlin | 206 | −38.6% | −38.6% | −38.6% | −75.3% | **7.5/10 good** |
| `.css` | CSS | 363 | −18.8% | −30.8% | −31.6% | −66.2% | **7/10 good** |
| `.py` | Python | 341 | −16.7% | −16.7% | −16.7% | −60.2% | **6/10 fair** |
| `.sql` | SQL | 261 | −18.6% | −18.6% | −18.6% | −42.2% | **6/10 fair** |
| `.sh` | Shell | 294 | −21.5% | −21.5% | −21.5% | −91.9% | **6/10 fair** |
| `.yml` | YAML | 312 | −21.9% | −21.9% | −21.9% | n/a | **5/10 fair** |

**Averages:** content-view −32.8%, apply −37.4%, rating **7.5/10**

### Real-corpus benchmark (45 languages, open-source files)

| Ext | Language | Bytes | Apply cut | Symbols cut | Rating |
|---|---|---:|---:|---:|---|
| `.dart` | Dart | 37 KB | −85.5% | n/a | **9.5/10 excellent** |
| `.scala` | Scala | 20 KB | −80.7% | −94.1% | **9.7/10 excellent** |
| `.proto` | Protocol Buffers | 60 KB | −69.1% | n/a | **9.5/10 excellent** |
| `.ts` | TypeScript | 92 KB | −67.9% | −69.1% | **9.7/10 excellent** |
| `.swift` | Swift | 34 KB | −65.5% | −81.3% | **9.7/10 excellent** |
| `.rb` | Ruby | 3.5 KB | −64.2% | −81.5% | **9.7/10 excellent** |
| `.rs` | Rust | 100 KB | −62.2% | −92.5% | **9.7/10 excellent** |
| `.java` | Java | 63 KB | −64.8% | −87.3% | **9.7/10 excellent** |
| `.php` | PHP | 35 KB | −41.4% | −87.1% | **9.4/10 excellent** |
| `.py` | Python | 34 KB | −21.9% | −85.3% | **8.6/10 strong** |
| `.kt` | Kotlin | 21 KB | −49.1% | −77.2% | **9/10 excellent** |
| `.css` | CSS | 280 KB | −17.9% | −67.9% | **8.5/10 strong** |

**Corpus summary:** 17 excellent · 12 strong · 13 good · 3 fair across 45 languages.
Average content-view cut **23.4%**, apply **31.3%**, async **32.9%**; symbols rating **9.2/10**.

### Why content-view and apply cuts differ

`applyContentViewMinification` preserves original indentation and line structure — that is intentional. It optimises for **agent readability**, not minimum bytes. `applyMinification` / `minifyContent` apply stronger compression (Terser for JS, TypeScript compiler + Terser for TS) that collapses whitespace and type annotations.

For TypeScript: content-view keeps the source shape (−25.9%); apply uses the full TypeScript→Terser pipeline (−61.4%). Both are correct for their intended contexts.

### Symbols mode: highest-value mode for large files

Symbols extraction consistently outperforms full minification for navigation tasks:

| Language | Apply cut | Symbols cut | Delta |
|---|---:|---:|---:|
| Shell | −21.5% | **−91.9%** | +70.4pp |
| Ruby | −55.4% | **−92.8%** | +37.4pp |
| JavaScript | −61.9% | **−91.3%** | +29.4pp |
| Java | −54.7% | **−83.3%** | +28.6pp |
| SQL | −18.6% | **−42.2%** | +23.6pp |

**Recommended workflow for large files (>5 KB):**
1. Call `extractSignatures` → read the skeleton and find the line range you need.
2. Fetch only that range with `startLine`/`endLine`.
3. Never request full minified content when the skeleton is sufficient.

---

## Architecture

```
Input (content + filePath)
        │
        ▼
  getFileConfig()          ← extension routing, 138 extensions
        │
        ▼
  Strategy dispatch
  ┌──────────┬──────────────┬──────────────┬────────┬──────────┐
  │  terser  │ conservative │  aggressive  │  json  │ markdown │
  │ JS/CJS   │ most langs   │ CSS/HTML/Lua │ JSON*  │  MD/RST  │
  └──────────┴──────────────┴──────────────┴────────┴──────────┘
        │
        ▼
  No-growth guard           ← returns original when output grows
        │
        ▼
  Output: shorter, agent-readable content
```

### Comment stripping is string-aware

The `conservative` strategy does not use bare regexes on comment syntax. Every comment family uses a character-level scanner that:

- Tracks open/close quote delimiters (including `"""`, `'''`, backtick, single/double quote)
- Handles language-specific escapes (backslash, doubling)
- Protects regexes in JS (`/pattern/g`) from being mistaken for comments
- Handles Rust raw strings (`r#"…"#`), C# verbatim strings (`@"…"`), PowerShell here-strings

Comment families handled: `c-style`, `hash`, `html`, `sql`, `lua`, `haskell`, `semicolon`, `wasm-text`, `percent`, `template`, `haml`, `slim`, `powershell`, `bang`, `apostrophe`, `double-dash`, `fsharp-block`, `pascal`, `python-docstring`.

### Python docstring stripping

Python `"""…"""` and `'''…'''` docstrings are stripped separately from `#` comments (which are line-based). The heuristic: a triple-quoted string whose opening delimiter is the first non-whitespace token on its line, immediately following a line that ends with `:` (function/class definition) or appears at the start of the file (module docstring).

Variable assignments like `query = """SELECT…"""` are not affected because the delimiter is not the first token.

### Signature extraction strategies

| Family | Languages | Approach |
|---|---|---|
| `tsJsStrategy` | `ts tsx js jsx mjs cjs` | TypeScript compiler API — pure parse, no type-check |
| `vueSvelteStrategy` | `vue svelte` | HTML scaffold + embedded script via `tsJsStrategy` |
| `pythonStrategy` | `py` | Regex heuristics: `def`/`class`/`import`/`@decorator` |
| `goStrategy` | `go` | `func`/`type`/`var`/`const`/`package` heuristics |
| `javaCsStrategy` | `java cs kt kotlin scala` | Class/interface/method head detection |
| `cFamilyStrategy` | `c h cpp hpp cc` | Preprocessor + struct/enum/class/function heuristics |
| `rustStrategy` | `rs rust` | `fn`/`struct`/`enum`/`impl`/`trait`/`pub` detection |
| `rubyStrategy` | `rb` | `def`/`class`/`module`/`attr_*`/`before_action` |
| `phpStrategy` | `php` | Function/class/interface/trait/namespace heads |
| `swiftStrategy` | `swift` | `func`/`class`/`struct`/`protocol`/`extension` |
| `cssStrategy` | `css scss less` | Selector + at-rule heads; declaration bodies dropped |
| `htmlStrategy` | `html htm` | Structural tags with `id=`/`class=`, heading elements |
| `sqlStrategy` | `sql` | `CREATE TABLE/VIEW/FUNCTION/PROCEDURE/INDEX` heads |
| `shellStrategy` | `sh bash zsh` | Function definitions + `export`/`source` lines |
| `scalaStrategy` | `scala` | `def`/`class`/`object`/`trait`/`val` heads |

---

## Development

### Project layout

```
src/
  core/
    strategies.ts      # All comment/whitespace/minification strategies
    minifier.ts        # Strategy dispatch + size guards
    apply.ts           # applyContentViewMinification, applyMinification
  signatures/
    extractSignatures.ts  # Per-language skeleton extractors
  types/
    index.ts           # MINIFY_CONFIG: 138 extension entries + comment patterns
  utils/
    fileExtension.ts   # Extension normalisation
  yaml/
    jsonToYamlString.ts  # YAML serialisation
  index.ts             # Public exports

tests/
  *.test.ts            # 1,399 tests
  languageBenchmarkFixtures.ts   # Synthetic fixtures for all 138 extensions
  largeSampleFixtures.ts         # ~400-line real code samples per language
  large-file-benchmark.test.ts   # Large-file benchmark runner

benchmark/
  {ext}/metrics.json         # Per-language real-corpus metrics
  {ext}/large-file-metrics.json  # Per-language large-file metrics
  large-files-summary.md     # Combined large-file benchmark report
  generate-real-code-report.mjs  # Regenerate from a local corpus
```

### Test commands

```bash
yarn test                          # All tests + v8 coverage (1,399 tests)
yarn test:quiet                    # Dot reporter, no output noise
yarn benchmark:quality             # Synthetic quality assertions (all 138 exts)

# Large-file benchmark (writes to benchmark/)
yarn vitest run tests/large-file-benchmark.test.ts

# Real-corpus report (requires a local corpus)
OCTOCODE_MINIFIER_REAL_BENCH_ROOTS=/path/to/repos \
  yarn vitest run tests/real-language-cut-report.test.ts --reporter=verbose

# Regenerate benchmark/ from a corpus
yarn build
node benchmark/generate-real-code-report.mjs /path/to/corpus
```

### Coverage

| File | Stmt | Branch | Lines |
|---|---|---|---|
| `apply.ts` | 100% | **100%** | 100% |
| `minifier.ts` | 100% | 95.4% | 100% |
| `strategies.ts` | 97.6% | 89.0% | 97.9% |
| `extractSignatures.ts` | 99.8% | 97.0% | 100% |
| `jsonToYamlString.ts` | 97.6% | 92.1% | 100% |
| **Total** | **99.0%** | **94.1%** | **99.3%** |

Uncovered lines are defensive last-resort catch blocks (e.g., component minifier outer catch, outer `minifyContent` catch). All inner helpers have their own try/catch and never propagate to these fallbacks in practice.

### Adding a new language

1. Add an entry to `MINIFY_CONFIG.fileTypes` in `src/types/index.ts`:
   ```ts
   myext: { strategy: 'conservative', comments: 'c-style' },
   ```
2. Run `yarn test` — the language quality benchmark auto-discovers all configured extensions.
3. Optionally add a signature strategy in `src/signatures/extractSignatures.ts` and register it in `STRATEGY_REGISTRY`.
4. Add a fixture entry in `tests/languageBenchmarkFixtures.ts` for richer quality assertions.

### Adding a comment pattern family

1. Add the name to `CommentPatternGroup` in `src/types/index.ts`.
2. Either:
   - Add regex patterns to `MINIFY_CONFIG.commentPatterns[name]` (simple cases), or
   - Add a case to `stringAwareRulesFor` in `src/core/strategies.ts` (string-aware, recommended for any language that uses the comment delimiter as a string character)
3. Wire the new family into any language configs that need it.

---

## Known limitations

| Language | Limitation | Workaround |
|---|---|---|
| Python | Docstrings in `if`/`for`/`while` bodies are stripped if that block is the first statement (prev line ends with `:`). Bare string expressions in control-flow bodies are valid but extremely rare. | Not a practical issue in real code |
| SQL | Stored procedure bodies are preserved in `conservative` mode — only `--` and `/* */` comments are stripped. Procedure body content survives until `symbols` mode. | Use `extractSignatures` for procedure navigation |
| YAML/JSON | No structural compression — these are data formats, not code. Token reduction comes entirely from comment stripping (YAML) or whitespace removal (JSON). | Use YAML output via `jsonToYamlString` for 15–30% compaction of equivalent JSON |
| Shell | Low content-view cut on logic-dense scripts with few comments. Symbols mode achieves −91%+ by keeping only function signatures and exports. | Use `extractSignatures` first, then `startLine`/`endLine` reads |
| `wasm` binary | Binary WebAssembly is not supported. Use a WAT-aware decompiler first. | `.wat`/`.wast` text format is supported |
| Files > 1 MB | Returned unchanged with `failed: true` in async mode. Sync mode has no size guard. | Pre-split large files before passing to the minifier |

---

## Competitor positioning

This package is an **agent-context compressor**, not a deploy-time optimizer.

| Tool | Best at | Octocode position |
|---|---|---|
| [Terser](https://www.npmjs.com/package/terser) | Production JS parsing, compression, mangling | Used internally for JS/CJS/MJS and TS→JS paths |
| [esbuild](https://www.npmjs.com/package/esbuild) | Fast JS/TS/CSS bundling | Better for production builds; not a runtime dep here |
| [SWC](https://www.npmjs.com/package/@swc/core) | Rust-backed JS/TS transforms | Better compiler-grade path; Octocode uses TS compiler + Terser |
| [Lightning CSS](https://www.npmjs.com/package/lightningcss) | Parser-grade CSS transforms | Better production CSS; Octocode uses CleanCSS for async |
| [html-minifier-terser](https://www.npmjs.com/package/html-minifier-terser) | HTML + embedded asset minification | Used internally for async HTML |

Use a production compiler when you need: executable output, source maps, tree shaking, cross-file dead-code elimination, or guaranteed semantic equivalence. Use this package when you need to pass many source files to an LLM cheaply and readably.
