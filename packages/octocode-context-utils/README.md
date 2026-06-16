# `@octocodeai/octocode-context-utils`

LLM context engine for AI agent context windows. It combines agent-readable compression, semantic signature extraction, deterministic pagination boundaries, and stable YAML serialization so file content, PR patches, and search results enter the model smaller while staying navigable.

> **Not a production build optimizer.** Use [Terser](https://www.npmjs.com/package/terser), [esbuild](https://www.npmjs.com/package/esbuild), or [Lightning CSS](https://www.npmjs.com/package/lightningcss) for deployable output. This package optimizes for agent-readable token reduction.

---

## Contents

- [Install & build](#install--build)
- [Quick start](#quick-start)
- [API reference](#api-reference)
- [Three modes](#three-modes)
- [Format support](#format-support)
- [C++/C# parser support](#cc-parser-support)
- [Benchmarks](#benchmarks)
- [Architecture](#architecture)
- [Development](#development)
- [Known limitations](#known-limitations)

---

## Install & build

```bash
yarn install
yarn build          # build the default native addon with parser-backed C++/C#
yarn verify         # typecheck + clippy + Rust tests + Node FFI tests + audit
```

Requirements: Node >= 20, Rust stable, and the napi-rs build toolchain.

---

## Quick start

```ts
import {
  applyContentViewMinification, // safe default for file reading tools
  applyMinification, // sync, keeps original on failure/growth
  minifyContent, // async, best available engine
  minifyContentSync, // sync raw strategy output
  extractSignatures, // navigation skeleton with line numbers
  jsonToYamlString, // YAML serialisation for MCP responses
} from '@octocodeai/octocode-context-utils';

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

**Use for:** MCP file-content tools, `localGetFileContent`, `ghGetFileContent`, PR patch context.

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

Dispatches to the strongest compiled engine for the format: OXC for JS/TS, Lightning CSS for CSS-family files, `minify-html` for HTML, and centralized strategy-backed cleanup otherwise. Returns:

```ts
type MinifyResult = {
  content: string;
  failed: boolean;
  type:
    | 'terser'
    | 'conservative'
    | 'aggressive'
    | 'json'
    | 'markdown'
    | 'general'
    | 'failed';
  reason?: string; // set on failure or degraded path
};
```

Files larger than 1 MB are returned unchanged with `failed: true`.

### `extractSignatures(content, filePath) → string | null`

**Use for:** "Show me the structure first" navigation workflows.

Returns a whole-file skeleton with the original 1-based line numbers in the left gutter. Bodies and comments are dropped. Markdown files return a document outline with headings, links, list anchors, frontmatter keys, and fenced-code language markers. Returns `null` when the extension is unsupported or extraction produces no results.

```txt
 1| import { serve } from 'bun';
 8| export interface Config {
12|   host: string;
14| }
17| export function createServer(config: Config): Server {
31| export class Router {
```

Supported extensions: `bash c cc cjs cpp cs css cxx erl ex exs go h hpp hrl hs htm html java js jsx kotlin kt less lhs lua markdown md mjs php plsql py rb rs scala scss sh sql svelte swift ts tsql tsx vue zsh`

`cpp`/`hpp`/`cc`/`cxx` and `cs` are supported in every build. The default npm artifact uses parser-backed signature extraction for these languages; `--no-default-features` keeps the heuristic fallback for slim experiments.

### `jsonToYamlString(obj, config?) → string`

Serialises any JSON-compatible value to YAML using Rust `serde_yaml_ng`. Multi-line string values are automatically converted to block scalars (`|-`) for readability. Accepts optional `sortKeys` and `keysPriority` for stable output ordering.

```ts
const yaml = jsonToYamlString({ name: 'read_file', path: 'src/app.ts' });
```

---

## Three modes

| Mode             | Function                              | Purpose                                                                                 | Output contract                                                                                                        |
| ---------------- | ------------------------------------- | --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| **content-view** | `applyContentViewMinification`        | Readable agent context — comment removal, whitespace compression, indentation preserved | Never grows. Not executable.                                                                                           |
| **minify**       | `minifyContent` / `minifyContentSync` | Maximum supported compression                                                           | Parser-backed for JS/TS/CSS/HTML; strategy-backed otherwise.                                                           |
| **symbols**      | `extractSignatures`                   | Navigation skeleton only                                                                | Parser-backed for configured grammars, heuristic fallback elsewhere. Lossy; use `startLine`/`endLine` to fetch bodies. |

---

## Format support

**138 registered extensions.** Every registered extension gets comment stripping appropriate for the language and whitespace normalisation.

### Parser / engine backed

Strongest compression path. Uses production parsing libraries.

| Extensions             | Strategy                        | Engine                                                     | Symbols |
| ---------------------- | ------------------------------- | ---------------------------------------------------------- | ------- |
| `js` `mjs` `cjs`       | terser                          | OXC parse + minify, JS fallback on parse failure           | ✓       |
| `jsx`                  | terser                          | OXC parse + minify, JS fallback on parse failure           | ✓       |
| `ts` `tsx`             | conservative + terser candidate | OXC parse + minify, TS-only syntax stripped                | ✓       |
| `css` `less` `scss`    | aggressive                      | Lightning CSS quality path, centralized text cleanup fallback | ✓       |
| `html` `htm`           | aggressive                      | `minify-html`, centralized text cleanup fallback           | ✓       |
| `vue` `svelte`         | aggressive                      | Component-aware heuristic skeletons + text cleanup         | ✓       |
| `json` `jsonc` `json5` | json                            | JSON parse → compact stringify; JSONC/JSON5 stripped first | —       |

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

| Extensions        | Behaviour                                                                                                                                                                                                                                                                                                                                                                                    |
| ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `md` `markdown`   | Strips HTML comments, pseudo-comments, generated TOCs, emoji glyphs/shortcodes, image-only/anchor/break-only lines, trailing whitespace, excessive blank lines, and soft-wrap paragraph newlines; preserves blockquotes, tables, fenced code, and ASCII data. Symbols mode returns a document outline with headings, links, list anchors, frontmatter keys, and fenced-code language markers |
| `txt` `log`       | Whitespace-only normalisation                                                                                                                                                                                                                                                                                                                                                                |
| unknown extension | General whitespace fallback                                                                                                                                                                                                                                                                                                                                                                  |

### Extensionless filenames

Routed through indentation-preserving conservative cleanup:

```
Makefile  Dockerfile  Procfile  Justfile  Rakefile  Gemfile
Podfile   Fastfile    Vagrantfile  Jenkinsfile  Cakefile
Pipfile   Buildfile   Capfile  Brewfile
```

---

## C++/C# Parser Support

C++ and C# signature extraction are supported in every build. The default package now enables parser-backed skeletons for `.cpp`, `.hpp`, `.cc`, `.cxx`, and `.cs` because the npm gzip cost is small compared with the quality gain.

The slim build still exists for size experiments and constrained distributions: compile with `--no-default-features` to fall back to heuristic C++/C# skeletons.

| Feature                      | Enables                                      | Disabled behavior            | Raw size cost |             Gzip size cost |
| ---------------------------- | -------------------------------------------- | ---------------------------- | ------------: | -------------------------: |
| `tree-sitter-cpp`            | Parser-backed `cpp` `hpp` `cc` `cxx` symbols | C-family heuristic skeletons |      ~3.3 MiB | included in combined total |
| `tree-sitter-c-sharp`        | Parser-backed `cs` symbols                   | Java/C# heuristic skeletons  |      ~5.1 MiB | included in combined total |
| `tree-sitter-large-grammars` | Both of the above                            | Heuristic fallback for both  |      ~8.4 MiB |                  ~0.63 MiB |

`tree-sitter-large-grammars` is the default feature group for the published package. It gives maintainers one stable switch for "build the full parser-backed C++/C# artifact" instead of remembering two feature names everywhere: publish jobs, benchmark comparisons, local size experiments, and regression checks can all use the same feature. The feature accepts the raw install-size cost to improve C++/C# body dropping, while adding only about `0.63 MiB` to the gzipped native addon.

Default native addon size with parser-backed C++/C# (`darwin-arm64`, OXC `0.135`, napi-rs v3):

```text
34.85 MiB raw
 5.62 MiB gzip
```

Slim native addon size with `--no-default-features` (`darwin-arm64`, OXC `0.135`, napi-rs v3):

```text
26.43 MiB raw
 4.99 MiB gzip
```

Default build:

```bash
yarn build
```

Slim build:

```bash
yarn napi build --platform --release --no-default-features
node scripts/postbuild.cjs
```

Feature-specific builds are still available for experiments:

```bash
yarn napi build --platform --release --no-default-features --features tree-sitter-cpp
yarn napi build --platform --release --no-default-features --features tree-sitter-c-sharp
```

For npm publishing, use the default feature set unless package-size policy changes. Cargo features are compile-time options; users cannot toggle them after installing the `.node` binary.

---

## Benchmarks

Reduction is byte reduction vs source. `n/a` means symbols are not available for that extension.

| Column         | API                            |
| -------------- | ------------------------------ |
| `content-view` | `applyContentViewMinification` |
| `apply`        | `applyMinification`            |
| `sync`         | `minifyContentSync`            |
| `async`        | `minifyContent`                |
| `symbols`      | `extractSignatures`            |

The C++ and C# `symbols` numbers use the default parser-backed build. Slim builds compiled with `--no-default-features` use heuristic extractors for those languages.

### Real corpus (`benchmark/*/metrics.json`)

| Ext        | Language         |  Source | content-view | apply |  sync | async | symbols |
| ---------- | ---------------- | ------: | -----------: | ----: | ----: | ----: | ------: |
| `.c`       | C                | 17.7 KB |         3.8% |  3.8% |  3.8% |  3.8% |   88.8% |
| `.cjs`     | CommonJS         |  3.1 KB |         4.8% | 49.7% | 49.7% | 49.7% |   97.8% |
| `.clj`     | Clojure          |  270 KB |         0.6% | 17.0% | 17.0% | 17.0% |     n/a |
| `.cpp`     | C++              | 31.9 KB |        30.2% | 30.2% | 30.2% | 30.2% |   80.6% |
| `.cs`      | C#               |  5.5 KB |        28.3% | 28.3% | 28.3% | 28.3% |   73.5% |
| `.css`     | CSS              |  274 KB |         0.4% | 15.3% | 15.3% | 17.9% |   67.9% |
| `.dart`    | Dart             | 36.2 KB |        85.5% | 85.5% | 85.5% | 85.5% |     n/a |
| `.erl`     | Erlang           |  120 KB |         5.8% | 23.6% | 23.6% | 23.6% |     n/a |
| `.ex`      | Elixir           |  151 KB |         1.5% | 16.2% | 16.2% | 16.2% |     n/a |
| `.go`      | Go               | 32.5 KB |        34.1% | 34.1% | 34.1% | 34.1% |   32.9% |
| `.graphql` | GraphQL          |  1.3 KB |         3.2% |  3.2% |  3.2% |  3.2% |     n/a |
| `.h`       | C Header         | 32.3 KB |        39.0% | 39.0% | 39.0% | 39.0% |   62.6% |
| `.hpp`     | C++ Header       | 24.7 KB |        38.3% | 38.3% | 38.3% | 38.3% |   71.5% |
| `.hs`      | Haskell          | 40.4 KB |        12.3% | 12.3% | 12.3% | 12.3% |     n/a |
| `.html`    | HTML             |  5.0 KB |         0.0% |  9.4% |  9.4% | 10.5% |   95.4% |
| `.ini`     | INI              |  7.3 KB |        23.6% | 23.6% | 23.6% | 23.6% |     n/a |
| `.java`    | Java             | 61.8 KB |        64.8% | 64.8% | 64.8% | 64.8% |   87.3% |
| `.js`      | JavaScript       |  6.7 KB |         9.6% | 21.6% | 21.6% | 21.6% |   61.3% |
| `.json`    | JSON             |  3.4 KB |         0.0% | 29.0% | 29.0% | 29.0% |     n/a |
| `.jsonc`   | JSONC            |  1.4 KB |         0.0% | 15.2% | 15.2% | 15.2% |     n/a |
| `.jsx`     | JSX              |  3.7 KB |         9.4% | 20.9% | 20.9% | 20.9% |   84.3% |
| `.kt`      | Kotlin           | 20.1 KB |        49.1% | 49.1% | 49.1% | 49.1% |   77.2% |
| `.lua`     | Lua              | 22.7 KB |        15.6% | 27.7% | 27.7% | 27.7% |     n/a |
| `.md`      | Markdown         |  3.2 KB |         1.2% |  1.2% |  1.2% |  1.2% |   40.6% |
| `.mjs`     | ESM JavaScript   |  1.2 KB |         1.5% | 31.2% | 31.2% | 31.2% |   78.7% |
| `.php`     | PHP              | 34.6 KB |        41.4% | 41.4% | 41.4% | 41.4% |   87.1% |
| `.pl`      | Perl             |  4.4 KB |        16.8% | 31.4% | 31.4% | 31.4% |     n/a |
| `.pm`      | Perl Module      |  5.4 KB |         8.9% | 20.1% | 20.1% | 20.1% |     n/a |
| `.proto`   | Protocol Buffers | 58.9 KB |        69.1% | 69.1% | 69.1% | 69.1% |     n/a |
| `.py`      | Python           | 64.2 KB |        21.3% | 21.3% | 21.3% | 21.3% |   63.6% |
| `.r`       | R                | 15.4 KB |        46.6% | 57.3% | 57.3% | 57.3% |     n/a |
| `.rb`      | Ruby             |  3.4 KB |        64.2% | 64.2% | 64.2% | 64.2% |   81.5% |
| `.rs`      | Rust             | 97.7 KB |        62.2% | 62.2% | 62.2% | 62.2% |   92.5% |
| `.rst`     | reStructuredText |  2.6 KB |         1.8% |  1.8% |  1.8% |  1.8% |     n/a |
| `.scala`   | Scala            | 19.6 KB |        80.7% | 80.7% | 80.7% | 80.7% |   94.1% |
| `.scss`    | SCSS             |  6.9 KB |        10.9% | 23.3% | 23.3% | 89.3% |   76.1% |
| `.sh`      | Shell            |  153 KB |         0.4% |  0.4% |  0.4% |  0.4% |   97.8% |
| `.sql`     | SQL              |  8.2 KB |        35.6% | 35.6% | 35.6% | 35.6% |   93.9% |
| `.svelte`  | Svelte           |  2.6 KB |         0.0% | 21.0% | 21.0% | 21.0% |   87.1% |
| `.swift`   | Swift            | 33.0 KB |        65.5% | 65.5% | 65.5% | 65.5% |   81.3% |
| `.toml`    | TOML             |  3.0 KB |        38.1% | 38.1% | 38.1% | 38.1% |     n/a |
| `.ts`      | TypeScript       | 90.3 KB |        29.6% | 67.9% | 67.9% | 67.9% |   69.1% |
| `.tsx`     | TSX              | 22.7 KB |        35.1% | 54.0% | 54.0% | 54.0% |   85.6% |
| `.vb`      | Visual Basic     | 88.9 KB |        10.1% | 10.1% | 10.1% | 10.1% |     n/a |
| `.vue`     | Vue              |  0.1 KB |         0.8% |  5.9% |  5.9% |  7.6% |   26.9% |
| `.yml`     | YAML             | 12.2 KB |         6.2% |  6.2% |  6.2% |  6.2% |     n/a |

### Large files (`benchmark/*/large-file-metrics.json`)

| Ext        | Language     | Lines |  Source | content-view | apply |  sync | async | symbols |
| ---------- | ------------ | ----: | ------: | -----------: | ----: | ----: | ----: | ------: |
| `.bash`    | Bash         |   235 |  7.1 KB |        39.9% | 39.9% | 39.9% | 39.9% |   94.1% |
| `.c`       | C            |   852 | 28.4 KB |        15.1% | 15.1% | 15.1% | 15.1% |   86.0% |
| `.cpp`     | C++          |   545 | 17.2 KB |         7.9% |  7.9% |  7.9% |  7.9% |   69.7% |
| `.cs`      | C#           |   226 |  8.3 KB |        33.0% | 33.0% | 33.0% | 33.0% |   81.9% |
| `.css`     | CSS          |   363 | 10.0 KB |        18.8% | 30.8% | 30.8% | 31.6% |   66.2% |
| `.go`      | Go           |   323 |  8.2 KB |        34.5% | 34.5% | 34.5% | 34.5% |   77.7% |
| `.graphql` | GraphQL      |   249 |  6.5 KB |        18.1% | 18.1% | 18.1% | 18.1% |     n/a |
| `.html`    | HTML         |   185 |  7.6 KB |        26.8% | 42.3% | 42.3% | 42.2% |   74.9% |
| `.java`    | Java         |   394 | 14.5 KB |        54.7% | 54.7% | 54.7% | 54.7% |   83.3% |
| `.js`      | JavaScript   |   419 | 10.4 KB |        54.1% | 61.9% | 61.9% | 61.9% |   91.3% |
| `.jsx`     | JSX          |   330 | 11.7 KB |        12.0% | 17.4% | 17.4% | 17.4% |   98.6% |
| `.kt`      | Kotlin       |   206 |  7.1 KB |        38.6% | 38.6% | 38.6% | 38.6% |   75.3% |
| `.md`      | Markdown     |   243 |  6.0 KB |        27.4% | 27.4% | 27.4% | 27.4% |     n/a |
| `.php`     | PHP          |   255 |  7.0 KB |        47.5% | 47.5% | 47.5% | 47.5% |   82.7% |
| `.proto`   | Protobuf     |   198 |  6.6 KB |        56.6% | 56.6% | 56.6% | 56.6% |     n/a |
| `.py`      | Python       |   341 | 10.5 KB |        16.7% | 16.7% | 16.7% | 16.7% |   60.2% |
| `.rb`      | Ruby         |   201 |  6.6 KB |        55.4% | 55.4% | 55.4% | 55.4% |   92.8% |
| `.rs`      | Rust         |   325 |  9.7 KB |        33.2% | 33.2% | 33.2% | 33.2% |   80.8% |
| `.scss`    | SCSS         |   291 |  6.9 KB |        28.2% | 40.6% | 40.6% | 70.4% |   55.7% |
| `.sh`      | Shell        |   294 |  8.9 KB |        21.5% | 21.5% | 21.5% | 21.5% |   91.9% |
| `.sql`     | SQL          |   261 |  8.8 KB |        18.6% | 18.6% | 18.6% | 18.6% |   42.2% |
| `.ts`      | TypeScript   |   322 |  9.9 KB |        25.9% | 61.4% | 61.4% | 61.4% |   71.3% |
| `.tsx`     | TSX          |   422 | 10.3 KB |         0.0% | 28.5% | 28.5% | 28.5% |   95.5% |
| `.vb`      | Visual Basic |   539 | 21.0 KB |         8.7% |  8.7% |  8.7% |  8.7% |     n/a |
| `.xml`     | XML          |   142 |  7.1 KB |        35.1% | 44.3% | 44.3% | 44.3% |     n/a |
| `.yml`     | YAML         |   297 | 10.1 KB |        67.6% | 67.6% | 67.6% | 67.6% |     n/a |

For large-file navigation, call `extractSignatures` first and fetch the needed line range.

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

This package is positioned as Octocode's LLM context engine, not as a generic minifier. The moat is semantic signatures, deterministic pagination helpers, and agent-readable compression under one native boundary.

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

| Family              | Languages                       | Approach                                                                |
| ------------------- | ------------------------------- | ----------------------------------------------------------------------- |
| `treeSitterTsJs`    | `ts tsx js jsx mjs cjs`         | Tree-sitter body dropping with heuristic fallback                       |
| `treeSitterCore`    | `py go rs java c h sh bash zsh` | Tree-sitter body dropping with heuristic fallback                       |
| `treeSitterLarge`   | `cpp hpp cc cxx cs`             | Default parser-backed C++/C# support; heuristic fallback in slim builds |
| `jvmHeuristic`      | `kt kotlin scala cs`            | Class/interface/method head detection                                   |
| `cFamilyHeuristic`  | `c h cpp hpp cc cxx`            | Preprocessor + struct/enum/class/function heuristics                    |
| `rubyHeuristic`     | `rb`                            | `def`/`class`/`module`/`attr_*` heads                                   |
| `phpHeuristic`      | `php`                           | Function/class/interface/trait/namespace heads                          |
| `swiftHeuristic`    | `swift`                         | `func`/`class`/`struct`/`protocol`/`extension`                          |
| `cssHeuristic`      | `css scss less`                 | Selector + at-rule heads; declaration bodies dropped                    |
| `htmlHeuristic`     | `html htm`                      | Structural tags with `id=`/`class=`, heading elements                   |
| `markdownHeuristic` | `md markdown`                   | Document outline: headings, links, list anchors, frontmatter keys       |
| `sqlHeuristic`      | `sql tsql plsql`                | `CREATE TABLE/VIEW/FUNCTION/PROCEDURE/INDEX` heads                      |
| `shellHeuristic`    | `sh bash zsh fish`              | Function definitions + `export`/`source` lines                          |

---

## Development

### Project layout

```
src/
  lib.rs               # napi-rs FFI exports only
  minifier.rs          # strategy dispatch + size guards
  apply.rs             # applyContentViewMinification, applyMinification
  strategies.rs        # comment/whitespace/parser-backed minification
  comment_remover.rs   # string-aware comment scanners
  config.rs            # extension routing table
  signatures/
    languages.rs       # tree-sitter grammar table + optional feature gates
    extractor.rs       # generic tree-sitter body-dropper
    heuristic.rs       # regex/brace-depth symbol fallbacks
    renderer.rs        # NNN| line-gutter output
  yaml_utils.rs        # YAML serialization
  types.rs             # napi object types

tests/
  ffi.test.ts          # Node/napi integration tests

benchmark/
  {ext}/metrics.json             # Per-language real-corpus metrics
  {ext}/large-file-metrics.json  # Per-language large-file metrics
  generate-real-code-report.mjs   # Regenerate benchmark docs from a corpus
```

### Daily workflow

```bash
yarn build          # release native addon, patch JS/types, sync current platform package
yarn build:dev      # debug/native addon for quick local iteration
yarn verify         # typecheck + clippy + cargo test + Node FFI tests + pack check + cargo audit

yarn test           # Rust unit tests
yarn test:node      # Node FFI tests, no coverage
yarn test:node:coverage
yarn lint:rust
yarn typecheck
yarn pack:check     # npm pack dry-run guard: no root .node files, bounded size
```

Run both default and slim grammar checks when touching signature extraction:

```bash
cargo check
cargo check --no-default-features
cargo test
yarn test:node
```

### Building grammar profiles

The package default includes `tree-sitter-cpp` and `tree-sitter-c-sharp` through `tree-sitter-large-grammars`.

Default parser-backed build:

```bash
yarn build
```

Slim build with heuristic C++/C# fallback:

```bash
yarn napi build --platform --release --no-default-features
node scripts/postbuild.cjs
```

Feature-specific builds are still useful for size attribution:

```bash
yarn napi build --platform --release --no-default-features --features tree-sitter-cpp
yarn napi build --platform --release --no-default-features --features tree-sitter-c-sharp
```

Keep `.cpp/.hpp/.cc/.cxx/.cs` listed in `getSupportedSignatureExtensions()` because the heuristic fallback supports them when parser grammars are disabled.

### Size checks

Measure the native addon after every dependency or grammar change:

```bash
yarn build
yarn pack:check
ls -lh *.node
gzip -c octocode-context-utils.*.node | wc -c
```

For crate-level code-size attribution:

```bash
cargo bloat --release --crates -n 40
cargo bloat --release -n 80
```

For constant/table-heavy changes, use a linker map. This is how parser table size was attributed:

```bash
rm -f /tmp/octocode-context-link.map
RUSTFLAGS='-C link-arg=-Wl,-map,/tmp/octocode-context-link.map' cargo build --release
```

V8 coverage only measures the JS loader/postbuild surface. Rust correctness lives in `cargo test`; native FFI behavior lives in `yarn test:node`.

### Adding a new language

1. Add an entry to `minify_config()` in `src/config.rs`:
   ```rust
   "myext" => FileTypeConfig::with("conservative", &["c-style"]),
   ```
2. Add or reuse a comment family in `src/comment_remover.rs`.
3. Add a heuristic extractor in `src/signatures/heuristic.rs` when `symbols` should work.
4. Add a tree-sitter grammar only when heuristic output is not good enough and the size cost is justified.
5. Run `cargo check`, `cargo test`, and `yarn test:node`.

### Adding an optional tree-sitter grammar

1. Add the grammar dependency as `optional = true` in `Cargo.toml`.
2. Add a named Cargo feature, for example `tree-sitter-my-lang = ["dep:tree-sitter-my-lang"]`.
3. Gate the language function, query, and `LANGUAGE_TABLE` entry with `#[cfg(feature = "...")]`.
4. Keep a heuristic fallback for the same extensions whenever possible.
5. Check both builds:
   ```bash
   cargo check
   cargo check --features tree-sitter-my-lang
   ```

### Adding a comment pattern family

1. Add scanner behavior in `src/comment_remover.rs`.
2. Wire the family into `src/config.rs`.
3. Add focused Rust tests for strings, escaped delimiters, and mixed comment styles.

### Publishing notes

This package publishes prebuilt `.node` artifacts through optional per-platform packages. The root npm package intentionally contains only the JS loaders, type declarations, metadata, and README; `yarn pack:check` fails if a native binary is accidentally included in the main tarball.

Every native build script runs `scripts/postbuild.cjs` after `napi build`. That script restores the generated CJS/type declaration additions and copies any built `octocode-context-utils.<triple>.node` file into its matching `npm/<platform>/` optional package directory. Release jobs should publish those platform packages first, then the root loader package.

Cargo features are compile-time, so npm users get whichever profile CI built. For `octocode-mcp`, publish the default parser-backed C++/C# profile; the measured gzip cost is small enough for the quality gain. Use `--no-default-features` only for constrained distributions or package-size experiments.

---

## Known limitations

| Language      | Limitation                                                                                                                                                                                        | Workaround                                                                      |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| Python        | Docstrings in `if`/`for`/`while` bodies are stripped if that block is the first statement (prev line ends with `:`). Bare string expressions in control-flow bodies are valid but extremely rare. | Not a practical issue in real code                                              |
| SQL           | Stored procedure bodies are preserved in `conservative` mode — only `--` and `/* */` comments are stripped. Procedure body content survives until `symbols` mode.                                 | Use `extractSignatures` for procedure navigation                                |
| YAML/JSON     | No structural compression — these are data formats, not code. Token reduction comes entirely from comment stripping (YAML) or whitespace removal (JSON).                                          | Use YAML output via `jsonToYamlString` for 15–30% compaction of equivalent JSON |
| Shell         | Low content-view cut on logic-dense scripts with few comments. Symbols mode achieves −91%+ by keeping only function signatures and exports.                                                       | Use `extractSignatures` first, then `startLine`/`endLine` reads                 |
| `wasm` binary | Binary WebAssembly is not supported. Use a WAT-aware decompiler first.                                                                                                                            | `.wat`/`.wast` text format is supported                                         |
| Files > 1 MB  | Returned unchanged. Result-returning APIs set `failed: true`.                                                                                                                                     | Pre-split large files before passing to the minifier                            |

---
