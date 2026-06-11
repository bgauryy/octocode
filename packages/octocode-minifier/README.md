# @octocodeai/octocode-minifier

Small, readable minification utilities for Octocode tool output.

This package is for agent context: file reads, PR patches, search results, and
structured responses. It tries to make content smaller while keeping enough
shape for an agent to understand and navigate it.

It is not a production compiler or bundler minifier. Use Terser, esbuild,
SWC, CleanCSS, or html-minifier-terser directly when you need deployable build
artifacts, source maps, tree shaking, or guaranteed semantic equivalence.

## Competitor Positioning

Octocode minifier competes as an agent-context compressor, not as a deploy-time
optimizer. The benchmark rating measures readable compression, no-growth
guards, useful symbol skeletons, and broad format coverage.

| Competitor | Best At | Octocode Position |
| --- | --- | --- |
| [Terser](https://www.npmjs.com/package/terser) | Production JavaScript parsing, compression, mangling, and formatting. | Used for JS/CJS/MJS and as a stronger JS-family backend where safe. |
| [esbuild](https://www.npmjs.com/package/esbuild) | Very fast JS/TS/CSS bundling and minification. | Better for production builds; Octocode avoids adding it as a runtime dependency. |
| [SWC](https://www.npmjs.com/package/@swc/core) | Rust-backed JS/TS compilation transforms. | Better compiler-grade path; Octocode keeps TS/TSX on a conservative contract with a TypeScript transform candidate when shorter. |
| [Lightning CSS](https://www.npmjs.com/package/lightningcss) | Parser-grade CSS transforms and minification. | Better production CSS optimizer; Octocode uses CleanCSS async and lightweight sync cleanup. |
| [html-minifier-terser](https://www.npmjs.com/package/html-minifier-terser) | HTML minification with embedded asset options. | Used for async HTML; content-view still prioritizes readable agent context. |

Quality target: if output must be executable, source-mapped, tree-shaken, or
cross-file optimized, use a production compiler/minifier. If output must help an
agent read many formats cheaply, use this package.

## Use It For

- `content-view`: readable compressed file content for agents.
- `minify`: stronger size reduction when parser or strategy support exists.
- `symbols`: line-numbered skeletons for navigating source files.
- `jsonToYamlString`: compact YAML output for structured MCP responses.

## Quick Start

```ts
import {
  applyContentViewMinification,
  applyMinification,
  extractSignatures,
  minifyContent,
  minifyContentSync,
} from '@octocodeai/octocode-minifier';

const readable = applyContentViewMinification(source, 'src/app.ts');
const syncCompact = applyMinification(source, 'src/app.ts');
const asyncCompact = await minifyContent(source, 'src/app.ts');
const skeleton = extractSignatures(source, 'src/app.ts');
```

## Which API Should I Use?

| Need | Use | Output |
| --- | --- | --- |
| Safe default for file-reading tools | `applyContentViewMinification` | Shorter, readable content. Returns original if shorter output is not useful. |
| Synchronous compact output | `applyMinification` | Calls the sync minifier and keeps the original on failure or growth. |
| Direct sync minification | `minifyContentSync` | Raw sync strategy result. |
| Best available engine-backed minification | `minifyContent` | Async result with `{ content, failed, type, reason }`. |
| Navigation-only source outline | `extractSignatures` | Whole-file skeleton with original line gutters. |
| YAML response formatting | `jsonToYamlString` | Stable YAML string. |

## Modes

| Mode | Best For | Guarantee |
| --- | --- | --- |
| `content-view` | MCP file content, local/GitHub fetch tools, PR context | Readable token reduction. Not executable-output safe. |
| `minify` | Smaller snippets and supported parser-backed formats | Shorter output where guarded helpers are used. Parser-backed only for some families. |
| `symbols` | "Show me the structure first" workflows | Lossy skeleton with line numbers. Use line ranges to fetch exact bodies. |

## Format Support

The package routes by file extension or known extensionless filenames.

### Parser Or Engine Backed

These paths have the strongest implementation:

| Format | Extensions | Minify Type | Symbols |
| --- | --- | --- | --- |
| JavaScript | `js`, `mjs`, `cjs` | Terser | Yes |
| JSX | `jsx` | TypeScript transform, then Terser when useful | Yes |
| TypeScript / TSX | `ts`, `tsx` | Conservative contract with TypeScript transform candidate when shorter | Yes |
| CSS | `css`, `less`, `scss` | CleanCSS async, lightweight sync cleanup | Yes |
| HTML | `html`, `htm` | html-minifier-terser async, lightweight sync cleanup | Yes |
| Vue / Svelte | `vue`, `svelte` | HTML cleanup plus embedded script/style minification | Yes |
| JSON family | `json`, `jsonc`, `json5` | JSON parse/cleanup/stringify | No |

### Readable Source Minification

These formats use conservative comment and whitespace cleanup. They keep line
structure and indentation instead of flattening code.

```txt
py, yaml, yml, coffee, nim, haml, slim, sass, styl,
go, java, c, h, cpp, hpp, cc, cs, rust, rs, swift, kt, kotlin, scala, dart,
groovy, gradle, mm, vb, vbs, pas, adb, ads, f, for, f90, f95, f03, f08,
zig, v, jl, nix,
php, rb, perl, sh, bash, zsh, fish, ps1, psm1, psd1,
sql, tsql, plsql, pls, pks, pkb, graphql, gql, proto,
csv, toml, ini, conf, config, env, properties,
tf, tfvars, pp, rst, star, bzl, cmake, fs, fsx, hs, lhs, elm,
xsl, xslt, awk, lisp, lsp, scm, rkt, vhd, vhdl, asm, nasm,
wat, wast, cfg, gitignore, dockerignore
```

### Lightweight Aggressive Cleanup

These formats get stronger text cleanup, but still not parser-grade compiler
minification:

```txt
lua, r, hbs, handlebars, ejs, mustache, twig, jinja, jinja2, erb,
pl, pm, clj, cljs, erl, hrl
```

### Markdown And Text

| Format | Extensions | Behavior |
| --- | --- | --- |
| Markdown | `md`, `markdown` | Removes HTML comments, quote-reply noise, trailing whitespace, and excessive blank lines. |
| Plain text | `txt`, `log` | General whitespace cleanup only. |
| Unknown extension | any unregistered extension | General whitespace cleanup fallback. |

### Extensionless Filenames

These names route through conservative indentation-preserving cleanup:

```txt
Makefile, Dockerfile, Procfile, Justfile, Rakefile, Gemfile, Podfile,
Fastfile, Vagrantfile, Jenkinsfile, Cakefile, Pipfile, Buildfile,
Capfile, Brewfile
```

## Symbols Support

`extractSignatures` returns `null` for unsupported formats. Supported
extensions:

```txt
bash, c, cc, cjs, cpp, cs, css, go, h, hpp, html, htm,
java, js, jsx, kotlin, kt, less, mjs, php, py, rb, rs, rust,
scala, scss, sh, sql, svelte, swift, ts, tsx, vue, zsh
```

Example output:

```txt
12| export function search(query: Query): Promise<Result> {
31| class SearchIndex {
```

Symbols are intentionally lossy. They are for navigation, not execution.

## Not Supported On Purpose

| Extension | Reason |
| --- | --- |
| `wasm` | Binary WebAssembly needs a binary-aware optimizer. |
| `m` | Ambiguous across incompatible source dialects. A single comment model would be unsafe. |
| `cob`, `cbl` | COBOL comments are column-sensitive. |
| `sas` | SAS supports statement-style comments that need a parser-aware pass. |
| `abap` | ABAP comments depend on statement position and string rules. |
| `bat`, `cmd` | Batch comments are command-parser-sensitive. |
| `s` | Assembly comment syntax varies by assembler and target dialect. |

WebAssembly text files, `wat` and `wast`, are supported as readable text
minification with `;;` and `(; ... ;)` comment handling. They do not have a WAT
parser or symbols mode.

## Quality Checks

Run from this package:

```bash
yarn lint
yarn typecheck
yarn test
yarn build
```

Run real-file quality checks against local repositories:

```bash
OCTOCODE_MINIFIER_REAL_BENCH_ROOTS=/path/to/repos yarn vitest run --reporter=verbose tests/real-language-cut-report.test.ts
OCTOCODE_MINIFIER_REAL_BENCH_ROOTS=/path/to/repos yarn vitest bench tests/real-language-files.bench.ts --run
```

The synthetic language tests cover every configured extension. The real-file
report checks that `content-view`, sync minification, and async minification do
not increase discovered file sizes.
