# Real-Code Minifier Benchmark

Generated from local corpus: `/tmp/octocode-minifier-real-corpus`

This directory records before/after excerpts and metrics for one real sample per
discovered extension. Full third-party source files are not vendored here; use
the generator to recreate reports from a local corpus.

## Summary

- Agent-context minifier rating: **8.46/10**
- Minify rating: **8.3/10**
- Agent understanding from minified output: **9.45/10**
- Agent usefulness across output levels: **9.46/10**
- Symbols rating where supported: **9.37/10** (27/27 returned)
- Average cuts: content-view 24.1%, apply 31.9%, async 33.4%
- Rating buckets: good 12, excellent 18, strong 13, fair 2, needs work 1

## Real README Minification Rating

Source: `md/rust-readme.md`

Overall README rating: **7.3/10 (good)**

| Dimension | Score / Value |
| --- | ---: |
| Input bytes | 3304 |
| Content-view bytes | 3303 |
| Content-view cut | 0% |
| Readability preservation | 10/10 |
| Byte reduction | 1/10 |

Signals: passed no growth, passed non-empty output, passed markdown strategy completed, passed headings preserved, passed links or references preserved, passed lists preserved.

- README score weights semantic preservation at 70% and byte reduction at 30%.
- Low byte cuts are acceptable when the README is already dense and mostly semantic content.
- Use this score to track whether Markdown changes preserve rendered README logic while still removing redundant tokens.


## Agent Understanding Quality From Minified Output

Measured from `content-view` output, which is the minified form intended for
agent context. Scores weight syntax anchors 40%, delimiter structure 20%,
output health 20%, context budget 10%, and symbol context 10%.

Rating buckets: excellent 42, strong 3, good 1

| Ext | Format | Understanding | Syntax anchors | Structure | Output health | Context budget | Symbol context | Signals |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `c` | C | 9.7/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 7/10 | 10/10 | 6/6 |
| `cjs` | CommonJS | 9.7/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 7/10 | 10/10 | 6/6 |
| `clj` | Clojure | 9.1/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 6/10 | 7/10 | 6/6 |
| `cpp` | C++ | 9.7/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 9/10 | 10/10 | 6/6 |
| `cs` | C# | 9.7/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 9/10 | 10/10 | 6/6 |
| `css` | CSS | 9.6/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 6/10 | 10/10 | 6/6 |
| `dart` | Dart | 9.5/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 8/10 | 7/10 | 6/6 |
| `erl` | Erlang | 9.4/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 7/10 | 7/10 | 6/6 |
| `ex` | Elixir | 9.3/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 6/10 | 7/10 | 6/6 |
| `go` | Go | 9.9/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 9/10 | 10/10 | 6/6 |
| `graphql` | GraphQL | 9.4/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 7/10 | 7/10 | 6/6 |
| `h` | C Header | 9.9/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 9/10 | 10/10 | 6/6 |
| `hpp` | C++ Header | 9.8/10 excellent | 10/10 (3/3) | 9.6/10 | 10/10 | 9/10 | 10/10 | 6/6 |
| `hs` | Haskell | 9.3/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 8/10 | 7/10 | 6/6 |
| `html` | HTML | 9.5/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 5/10 | 10/10 | 6/6 |
| `ini` | INI | 9.5/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 8/10 | 7/10 | 6/6 |
| `java` | Java | 9.8/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 10/10 | 10/10 | 6/6 |
| `js` | JavaScript | 9.5/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 7/10 | 10/10 | 6/6 |
| `json` | JSON | 9.2/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 5/10 | 7/10 | 6/6 |
| `jsonc` | JSONC | 9.2/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 5/10 | 7/10 | 6/6 |
| `jsx` | JSX | 9.7/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 7/10 | 10/10 | 6/6 |
| `kt` | Kotlin | 9.8/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 10/10 | 10/10 | 6/6 |
| `lua` | Lua | 9.5/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 8/10 | 7/10 | 6/6 |
| `md` | Markdown | 9.2/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 5/10 | 7/10 | 6/6 |
| `mjs` | ESM JavaScript | 9.6/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 6/10 | 10/10 | 6/6 |
| `php` | PHP | 9.7/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 9/10 | 10/10 | 6/6 |
| `pl` | Perl | 8.2/10 strong | 6.7/10 (2/3) | 10/10 | 10/10 | 8/10 | 7/10 | 6/6 |
| `pm` | Perl Module | 8.1/10 strong | 6.7/10 (2/3) | 10/10 | 10/10 | 7/10 | 7/10 | 6/6 |
| `proto` | Protocol Buffers | 9.7/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 10/10 | 7/10 | 6/6 |
| `py` | Python | 9.8/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 8/10 | 10/10 | 6/6 |
| `r` | R | 9.5/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 10/10 | 7/10 | 6/6 |
| `rb` | Ruby | 10/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 10/10 | 10/10 | 6/6 |
| `rs` | Rust | 10/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 10/10 | 10/10 | 6/6 |
| `rst` | reStructuredText | 8/10 strong | 6.7/10 (2/3) | 10/10 | 10/10 | 6/10 | 7/10 | 6/6 |
| `scala` | Scala | 9.6/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 8/10 | 10/10 | 6/6 |
| `scss` | SCSS | 9.6/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 8/10 | 10/10 | 6/6 |
| `sh` | Shell | 9.3/10 excellent | 10/10 (3/3) | 9.4/10 | 9/10 | 6/10 | 10/10 | 6/6 |
| `sql` | SQL | 9.7/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 9/10 | 10/10 | 6/6 |
| `svelte` | Svelte | 9.5/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 5/10 | 10/10 | 6/6 |
| `swift` | Swift | 10/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 10/10 | 10/10 | 6/6 |
| `toml` | TOML | 9.6/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 9/10 | 7/10 | 6/6 |
| `ts` | TypeScript | 9.7/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 9/10 | 10/10 | 6/6 |
| `tsx` | TSX | 9.7/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 9/10 | 10/10 | 6/6 |
| `vb` | Visual Basic | 9.5/10 excellent | 10/10 (3/3) | 10/10 | 10/10 | 8/10 | 7/10 | 6/6 |
| `vue` | Vue | 7.9/10 good | 6.7/10 (2/3) | 8/10 | 10/10 | 6/10 | 10/10 | 6/6 |
| `yml` | YAML | 9.2/10 excellent | 10/10 (3/3) | 10/10 | 9/10 | 7/10 | 7/10 | 6/6 |

## Agent Observation By Output Level

Measured from the actual raw and generated outputs for each language. `none`
is exact source fidelity, `standard` is the default agent-readable minified
view, `minify` is the full async minifier output, and `symbols` is the
navigation skeleton when available.

| Level | Samples | Avg score | Avg cut | Buckets |
| --- | ---: | ---: | ---: | --- |
| none | 46 | 10/10 | 0% | excellent 46 |
| standard | 46 | 9.45/10 | 24.06% | excellent 42, strong 3, good 1 |
| minify | 46 | 9.42/10 | 33.43% | excellent 38, strong 8 |
| symbols | 27 | 8.43/10 | 77.64% | strong 13, excellent 9, fair 2, good 3 |

| Ext | Format | none | standard | minify | symbols |
| --- | --- | ---: | ---: | ---: | ---: |
| `c` | C | 10/10 excellent (0%) | 9.7/10 excellent (3.8%) | 9.7/10 excellent (3.8%) | 8.5/10 strong (88.8%) |
| `cjs` | CommonJS | 10/10 excellent (0%) | 9.7/10 excellent (4.8%) | 10/10 excellent (49.7%) | 9.8/10 excellent (97.8%) |
| `clj` | Clojure | 10/10 excellent (0%) | 9.1/10 excellent (0.6%) | 8/10 strong (17%) | n/a |
| `cpp` | C++ | 10/10 excellent (0%) | 9.7/10 excellent (30.2%) | 9.7/10 excellent (30.2%) | 9/10 excellent (80.6%) |
| `cs` | C# | 10/10 excellent (0%) | 9.7/10 excellent (28.3%) | 9.7/10 excellent (28.3%) | 8.5/10 strong (73.5%) |
| `css` | CSS | 10/10 excellent (0%) | 9.6/10 excellent (0.4%) | 9.8/10 excellent (17.9%) | 6.7/10 fair (67.9%) |
| `dart` | Dart | 10/10 excellent (0%) | 9.5/10 excellent (85.5%) | 9.5/10 excellent (85.5%) | n/a |
| `erl` | Erlang | 10/10 excellent (0%) | 9.4/10 excellent (5.8%) | 9.5/10 excellent (23.6%) | n/a |
| `ex` | Elixir | 10/10 excellent (0%) | 9.3/10 excellent (1.5%) | 9.5/10 excellent (16.2%) | n/a |
| `go` | Go | 10/10 excellent (0%) | 9.9/10 excellent (34.1%) | 9.9/10 excellent (34.1%) | 9.9/10 excellent (32.9%) |
| `graphql` | GraphQL | 10/10 excellent (0%) | 9.4/10 excellent (3.2%) | 9.4/10 excellent (3.2%) | n/a |
| `h` | C Header | 10/10 excellent (0%) | 9.9/10 excellent (39%) | 9.9/10 excellent (39%) | 9.6/10 excellent (62.6%) |
| `hpp` | C++ Header | 10/10 excellent (0%) | 9.8/10 excellent (38.3%) | 9.8/10 excellent (38.3%) | 9.2/10 excellent (71.5%) |
| `hs` | Haskell | 10/10 excellent (0%) | 9.3/10 excellent (12.3%) | 9.3/10 excellent (12.3%) | n/a |
| `html` | HTML | 10/10 excellent (0%) | 9.5/10 excellent (0%) | 9.8/10 excellent (10.5%) | 8.1/10 strong (95.4%) |
| `ini` | INI | 10/10 excellent (0%) | 9.5/10 excellent (23.6%) | 9.5/10 excellent (23.6%) | n/a |
| `java` | Java | 10/10 excellent (0%) | 9.8/10 excellent (64.8%) | 9.8/10 excellent (64.8%) | 7.8/10 good (87.3%) |
| `js` | JavaScript | 10/10 excellent (0%) | 9.5/10 excellent (9.6%) | 9.6/10 excellent (21.6%) | 9.3/10 excellent (61.3%) |
| `json` | JSON | 10/10 excellent (0%) | 9.2/10 excellent (0%) | 9.6/10 excellent (29%) | n/a |
| `jsonc` | JSONC | 10/10 excellent (0%) | 9.2/10 excellent (0%) | 9.5/10 excellent (15.2%) | n/a |
| `jsx` | JSX | 10/10 excellent (0%) | 9.7/10 excellent (9.4%) | 8.5/10 strong (20.9%) | 8.2/10 strong (84.3%) |
| `kt` | Kotlin | 10/10 excellent (0%) | 9.8/10 excellent (49.1%) | 9.8/10 excellent (49.1%) | 8/10 strong (77.2%) |
| `lua` | Lua | 10/10 excellent (0%) | 9.5/10 excellent (15.6%) | 9.6/10 excellent (27.7%) | n/a |
| `md` | Markdown | 10/10 excellent (0%) | 9.2/10 excellent (0%) | 9.2/10 excellent (0%) | n/a |
| `mjs` | ESM JavaScript | 10/10 excellent (0%) | 9.6/10 excellent (1.5%) | 9.9/10 excellent (31.2%) | 9/10 excellent (78.7%) |
| `php` | PHP | 10/10 excellent (0%) | 9.7/10 excellent (41.4%) | 9.7/10 excellent (41.4%) | 8.3/10 strong (87.1%) |
| `pl` | Perl | 10/10 excellent (0%) | 8.2/10 strong (16.8%) | 8.3/10 strong (31.4%) | n/a |
| `pm` | Perl Module | 10/10 excellent (0%) | 8.1/10 strong (8.9%) | 8.2/10 strong (20.1%) | n/a |
| `proto` | Protocol Buffers | 10/10 excellent (0%) | 9.7/10 excellent (69.1%) | 9.7/10 excellent (69.1%) | n/a |
| `py` | Python | 10/10 excellent (0%) | 9.8/10 excellent (21.3%) | 9.8/10 excellent (21.3%) | 8.7/10 strong (63.6%) |
| `r` | R | 10/10 excellent (0%) | 9.5/10 excellent (46.6%) | 9.5/10 excellent (57.3%) | n/a |
| `rb` | Ruby | 10/10 excellent (0%) | 10/10 excellent (64.2%) | 10/10 excellent (64.2%) | 8.1/10 strong (81.5%) |
| `rs` | Rust | 10/10 excellent (0%) | 10/10 excellent (62.2%) | 10/10 excellent (62.2%) | 9.2/10 excellent (92.5%) |
| `rst` | reStructuredText | 10/10 excellent (0%) | 8/10 strong (1.8%) | 8/10 strong (1.8%) | n/a |
| `scala` | Scala | 10/10 excellent (0%) | 9.6/10 excellent (80.7%) | 9.6/10 excellent (80.7%) | 8.9/10 strong (94.1%) |
| `scss` | SCSS | 10/10 excellent (0%) | 9.6/10 excellent (10.9%) | 8.5/10 strong (89.3%) | 8.1/10 strong (76.1%) |
| `sh` | Shell | 10/10 excellent (0%) | 9.3/10 excellent (0.4%) | 9.3/10 excellent (0.4%) | 6.1/10 fair (97.8%) |
| `sql` | SQL | 10/10 excellent (0%) | 9.7/10 excellent (35.6%) | 9.7/10 excellent (35.6%) | 7.1/10 good (93.9%) |
| `svelte` | Svelte | 10/10 excellent (0%) | 9.5/10 excellent (0%) | 9.8/10 excellent (21%) | 8.1/10 strong (87.1%) |
| `swift` | Swift | 10/10 excellent (0%) | 10/10 excellent (65.5%) | 10/10 excellent (65.5%) | 7.6/10 good (81.3%) |
| `toml` | TOML | 10/10 excellent (0%) | 9.6/10 excellent (38.1%) | 9.6/10 excellent (38.1%) | n/a |
| `ts` | TypeScript | 10/10 excellent (0%) | 9.7/10 excellent (29.6%) | 9.8/10 excellent (67.9%) | 9.3/10 excellent (69.1%) |
| `tsx` | TSX | 10/10 excellent (0%) | 9.7/10 excellent (35.1%) | 8.5/10 strong (54%) | 8.2/10 strong (85.6%) |
| `vb` | Visual Basic | 10/10 excellent (0%) | 9.5/10 excellent (10.1%) | 9.5/10 excellent (10.1%) | n/a |
| `vue` | Vue | 10/10 excellent (0%) | 7.9/10 good (0.8%) | 8/10 strong (7.6%) | 8.2/10 strong (26.9%) |
| `yml` | YAML | 10/10 excellent (0%) | 9.2/10 excellent (6.2%) | 9.2/10 excellent (6.2%) | n/a |

## Competitor Baseline

This benchmark rates Octocode as an agent-context compressor. Production
compiler and bundler minifiers are the right baseline for deployable output:

| Competitor | Best At | Octocode Position |
| --- | --- | --- |
| [Terser](https://www.npmjs.com/package/terser) | Production JavaScript parsing, compression, mangling, and formatting. | Used for JS/CJS/MJS and stronger JS-family paths where safe. |
| [esbuild](https://www.npmjs.com/package/esbuild) | Very fast JS/TS/CSS bundling and minification. | Better for production builds; Octocode avoids adding it as a runtime dependency. |
| [SWC](https://www.npmjs.com/package/@swc/core) | Rust-backed JS/TS compilation transforms. | Better compiler-grade path; Octocode uses TypeScript transform plus guarded minification. |
| [Lightning CSS](https://www.npmjs.com/package/lightningcss) | Parser-grade CSS transforms and minification. | Better production CSS optimizer; Octocode uses CleanCSS async and lightweight sync cleanup. |
| [html-minifier-terser](https://www.npmjs.com/package/html-minifier-terser) | HTML minification with embedded asset options. | Used for async HTML; content-view still prioritizes readable agent context. |

## Real Minification Type Matrix

Measured async result types across the real corpus: conservative 27, terser 4, aggressive 12, json 2, markdown 1.

| Ext | Format | Configured strategy | Async type | Content-view cut | Apply cut | Async cut | Symbols cut | Source |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | --- |
| `js` | JavaScript | `terser` | `terser` | 9.6% | 21.6% | 21.6% | 61.3% | `js/00-react-hooks.js` |
| `cjs` | CommonJS | `terser` | `terser` | 4.8% | 49.7% | 49.7% | 97.8% | `cjs/apidom-babel.config.cjs` |
| `mjs` | ESM JavaScript | `terser` | `terser` | 1.5% | 31.2% | 31.2% | 78.7% | `mjs/llhttp-eslint.config.mjs` |
| `jsx` | JSX | `terser` | `terser` | 9.4% | 20.9% | 20.9% | 84.3% | `jsx/00-fullcalendar-demo.jsx` |
| `ts` | TypeScript | `conservative` | `conservative` | 29.6% | 67.9% | 67.9% | 69.1% | `ts/00-typescript-core.ts` |
| `tsx` | TSX | `conservative` | `conservative` | 35.1% | 54% | 54% | 85.6% | `tsx/00-next-app-router.tsx` |
| `json` | JSON | `json` | `json` | 0% | 29% | 29% | n/a | `json/typescript-package.json` |
| `jsonc` | JSONC | `json` | `json` | 0% | 15.2% | 15.2% | n/a | `jsonc/grammy-deno.jsonc` |
| `css` | CSS | `aggressive` | `aggressive` | 0.4% | 15.3% | 17.9% | 67.9% | `css/bootstrap.css` |
| `scss` | SCSS | `aggressive` | `aggressive` | 10.9% | 23.3% | 89.3% | 76.1% | `scss/_buttons.scss` |
| `html` | HTML | `aggressive` | `aggressive` | 0% | 9.4% | 10.5% | 95.4% | `html/00-mdn-letter.html` |
| `vue` | Vue | `aggressive` | `aggressive` | 0.8% | 5.9% | 7.6% | 26.9% | `vue/vite-app.vue` |
| `svelte` | Svelte | `aggressive` | `aggressive` | 0% | 21% | 21% | 87.1% | `svelte/vite-app.svelte` |
| `py` | Python | `conservative` | `conservative` | 21.3% | 21.3% | 21.3% | 63.6% | `py/00-httpx-client.py` |
| `java` | Java | `conservative` | `conservative` | 64.8% | 64.8% | 64.8% | 87.3% | `java/00-spring-annotation-utils.java` |
| `go` | Go | `conservative` | `conservative` | 34.1% | 34.1% | 34.1% | 32.9% | `go/print.go` |
| `rs` | Rust | `conservative` | `conservative` | 62.2% | 62.2% | 62.2% | 92.5% | `rs/option.rs` |
| `c` | C | `conservative` | `conservative` | 3.8% | 3.8% | 3.8% | 88.8% | `c/00-git-add.c` |
| `cpp` | C++ | `conservative` | `conservative` | 30.2% | 30.2% | 30.2% | 80.6% | `cpp/00-llvm-raw-ostream.cpp` |
| `h` | C Header | `conservative` | `conservative` | 39% | 39% | 39% | 62.6% | `h/git-compat-util.h` |
| `hpp` | C++ Header | `conservative` | `conservative` | 38.3% | 38.3% | 38.3% | 71.5% | `hpp/fmt-color.hpp` |
| `cs` | C# | `conservative` | `conservative` | 28.3% | 28.3% | 28.3% | 73.5% | `cs/00-dotnet-argument-exception.cs` |
| `php` | PHP | `conservative` | `conservative` | 41.4% | 41.4% | 41.4% | 87.1% | `php/Arr.php` |
| `rb` | Ruby | `conservative` | `conservative` | 64.2% | 64.2% | 64.2% | 81.5% | `rb/blank.rb` |
| `sh` | Shell | `conservative` | `conservative` | 0.4% | 0.4% | 0.4% | 97.8% | `sh/nvm.sh` |
| `sql` | SQL | `conservative` | `conservative` | 35.6% | 35.6% | 35.6% | 93.9% | `sql/00-postgres-select.sql` |
| `yml` | YAML | `conservative` | `conservative` | 6.2% | 6.2% | 6.2% | n/a | `yaml/typescript-ci.yml` |
| `toml` | TOML | `conservative` | `conservative` | 38.1% | 38.1% | 38.1% | n/a | `toml/rust-cargo.toml` |
| `lua` | Lua | `aggressive` | `aggressive` | 15.6% | 27.7% | 27.7% | n/a | `lua/plenary-path.lua` |
| `graphql` | GraphQL | `conservative` | `conservative` | 3.2% | 3.2% | 3.2% | n/a | `graphql/graphql-go-kitchen-sink.graphql` |
| `md` | Markdown | `markdown` | `markdown` | 0% | 0% | 0% | n/a | `md/rust-readme.md` |
| `rst` | reStructuredText | `conservative` | `conservative` | 1.8% | 1.8% | 1.8% | n/a | `rst/cpython-tutorial-index.rst` |
| `scala` | Scala | `conservative` | `conservative` | 80.7% | 80.7% | 80.7% | 94.1% | `scala/Option.scala` |
| `swift` | Swift | `conservative` | `conservative` | 65.5% | 65.5% | 65.5% | 81.3% | `swift/Optional.swift` |
| `kt` | Kotlin | `conservative` | `conservative` | 49.1% | 49.1% | 49.1% | 77.2% | `kt/Collections.kt` |
| `dart` | Dart | `conservative` | `conservative` | 85.5% | 85.5% | 85.5% | n/a | `dart/dart-string.dart` |
| `r` | R | `aggressive` | `aggressive` | 46.6% | 57.3% | 57.3% | n/a | `r/dplyr-mutate.R` |
| `proto` | Protocol Buffers | `conservative` | `conservative` | 69.1% | 69.1% | 69.1% | n/a | `proto/protobuf-descriptor.proto` |

| Ext | Format | Report | Input bytes | Content-view cut | Apply cut | Sync cut | Async cut | Symbols cut | Agent rating |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `c` | C | `c/README.md` | 18107 | 3.8% | 3.8% | 3.8% | 3.8% | 88.8% | 7.6/10 good |
| `cjs` | CommonJS | `cjs/README.md` | 3184 | 4.8% | 49.7% | 49.7% | 49.7% | 97.8% | 9.7/10 excellent |
| `clj` | Clojure | `clj/README.md` | 276207 | 0.6% | 17% | 17% | 17% | n/a | 7.8/10 good |
| `cpp` | C++ | `cpp/README.md` | 32621 | 30.2% | 30.2% | 30.2% | 30.2% | 80.6% | 9/10 excellent |
| `cs` | C# | `cs/README.md` | 5603 | 28.3% | 28.3% | 28.3% | 28.3% | 73.5% | 8.7/10 strong |
| `css` | CSS | `css/README.md` | 280311 | 0.4% | 15.3% | 15.3% | 17.9% | 67.9% | 8.5/10 strong |
| `dart` | Dart | `dart/README.md` | 37049 | 85.5% | 85.5% | 85.5% | 85.5% | n/a | 9.5/10 excellent |
| `erl` | Erlang | `erl/README.md` | 123312 | 5.8% | 23.6% | 23.6% | 23.6% | n/a | 7.8/10 good |
| `ex` | Elixir | `ex/README.md` | 154291 | 1.5% | 16.2% | 16.2% | 16.2% | n/a | 7.8/10 good |
| `go` | Go | `go/README.md` | 33315 | 34.1% | 34.1% | 34.1% | 34.1% | 32.9% | 7.8/10 good |
| `graphql` | GraphQL | `graphql/README.md` | 1300 | 3.2% | 3.2% | 3.2% | 3.2% | n/a | 6.3/10 fair |
| `h` | C Header | `h/README.md` | 33059 | 39% | 39% | 39% | 39% | 62.6% | 8.7/10 strong |
| `hpp` | C++ Header | `hpp/README.md` | 25322 | 38.3% | 38.3% | 38.3% | 38.3% | 71.5% | 8.7/10 strong |
| `hs` | Haskell | `hs/README.md` | 41400 | 12.3% | 12.3% | 12.3% | 12.3% | n/a | 7/10 good |
| `html` | HTML | `html/README.md` | 5096 | 0% | 9.4% | 9.4% | 10.5% | 95.4% | 8.4/10 strong |
| `ini` | INI | `ini/README.md` | 7518 | 23.6% | 23.6% | 23.6% | 23.6% | n/a | 7.8/10 good |
| `java` | Java | `java/README.md` | 63265 | 64.8% | 64.8% | 64.8% | 64.8% | 87.3% | 9.7/10 excellent |
| `js` | JavaScript | `js/README.md` | 6864 | 9.6% | 21.6% | 21.6% | 21.6% | 61.3% | 8.5/10 strong |
| `json` | JSON | `json/README.md` | 3468 | 0% | 29% | 29% | 29% | n/a | 9/10 excellent |
| `jsonc` | JSONC | `jsonc/README.md` | 1427 | 0% | 15.2% | 15.2% | 15.2% | n/a | 8.3/10 strong |
| `jsx` | JSX | `jsx/README.md` | 3825 | 9.4% | 20.9% | 20.9% | 20.9% | 84.3% | 8.9/10 strong |
| `kt` | Kotlin | `kt/README.md` | 20559 | 49.1% | 49.1% | 49.1% | 49.1% | 77.2% | 9/10 excellent |
| `lua` | Lua | `lua/README.md` | 23250 | 15.6% | 27.7% | 27.7% | 27.7% | n/a | 8.5/10 strong |
| `md` | Markdown | `md/README.md` | 3304 | 0% | 0% | 0% | 0% | n/a | 5.5/10 needs work |
| `mjs` | ESM JavaScript | `mjs/README.md` | 1259 | 1.5% | 31.2% | 31.2% | 31.2% | 78.7% | 9/10 excellent |
| `php` | PHP | `php/README.md` | 35469 | 41.4% | 41.4% | 41.4% | 41.4% | 87.1% | 9.4/10 excellent |
| `pl` | Perl | `pl/README.md` | 4523 | 16.8% | 31.4% | 31.4% | 31.4% | n/a | 8.5/10 strong |
| `pm` | Perl Module | `pm/README.md` | 5491 | 8.9% | 20.1% | 20.1% | 20.1% | n/a | 7.8/10 good |
| `proto` | Protocol Buffers | `proto/README.md` | 60347 | 69.1% | 69.1% | 69.1% | 69.1% | n/a | 9.5/10 excellent |
| `py` | Python | `py/README.md` | 65713 | 21.3% | 21.3% | 21.3% | 21.3% | 63.6% | 8.2/10 strong |
| `r` | R | `r/README.md` | 15796 | 46.6% | 57.3% | 57.3% | 57.3% | n/a | 9/10 excellent |
| `rb` | Ruby | `rb/README.md` | 3507 | 64.2% | 64.2% | 64.2% | 64.2% | 81.5% | 9.7/10 excellent |
| `rs` | Rust | `rs/README.md` | 100057 | 62.2% | 62.2% | 62.2% | 62.2% | 92.5% | 9.7/10 excellent |
| `rst` | reStructuredText | `rst/README.md` | 2616 | 1.8% | 1.8% | 1.8% | 1.8% | n/a | 6.3/10 fair |
| `scala` | Scala | `scala/README.md` | 20107 | 80.7% | 80.7% | 80.7% | 80.7% | 94.1% | 9.7/10 excellent |
| `scss` | SCSS | `scss/README.md` | 7057 | 10.9% | 23.3% | 23.3% | 89.3% | 76.1% | 9.7/10 excellent |
| `sh` | Shell | `sh/README.md` | 156857 | 0.4% | 0.4% | 0.4% | 0.4% | 97.8% | 7.6/10 good |
| `sql` | SQL | `sql/README.md` | 8415 | 35.6% | 35.6% | 35.6% | 35.6% | 93.9% | 9/10 excellent |
| `svelte` | Svelte | `svelte/README.md` | 2665 | 0% | 21% | 21% | 21% | 87.1% | 8.9/10 strong |
| `swift` | Swift | `swift/README.md` | 33805 | 65.5% | 65.5% | 65.5% | 65.5% | 81.3% | 9.7/10 excellent |
| `toml` | TOML | `toml/README.md` | 3039 | 38.1% | 38.1% | 38.1% | 38.1% | n/a | 8.5/10 strong |
| `ts` | TypeScript | `ts/README.md` | 92419 | 29.6% | 67.9% | 67.9% | 67.9% | 69.1% | 9.7/10 excellent |
| `tsx` | TSX | `tsx/README.md` | 23197 | 35.1% | 54% | 54% | 54% | 85.6% | 9.7/10 excellent |
| `vb` | Visual Basic | `vb/README.md` | 91031 | 10.1% | 10.1% | 10.1% | 10.1% | n/a | 7/10 good |
| `vue` | Vue | `vue/README.md` | 119 | 0.8% | 5.9% | 5.9% | 7.6% | 26.9% | 7.2/10 good |
| `yml` | YAML | `yml/README.md` | 12508 | 6.2% | 6.2% | 6.2% | 6.2% | n/a | 7/10 good |

## Coverage

- Configured extensions: 138
- Real corpus extensions covered here: 46
- Configured extensions missing from this corpus: 92

See `missing-real-samples.md` for formats that are
supported by the package but not present in this local corpus.

## Weakest Measured Formats

- `.md` Markdown: 5.5/10 needs work; content-view cut 0%, async cut 0%, symbols not returned.
- `.graphql` GraphQL: 6.3/10 fair; content-view cut 3.2%, async cut 3.2%, symbols not returned.
- `.rst` reStructuredText: 6.3/10 fair; content-view cut 1.8%, async cut 1.8%, symbols not returned.
- `.hs` Haskell: 7/10 good; content-view cut 12.3%, async cut 12.3%, symbols not returned.
- `.vb` Visual Basic: 7/10 good; content-view cut 10.1%, async cut 10.1%, symbols not returned.
- `.yml` YAML: 7/10 good; content-view cut 6.2%, async cut 6.2%, symbols not returned.
- `.vue` Vue: 7.2/10 good; content-view cut 0.8%, async cut 7.6%, symbols returned.
- `.c` C: 7.6/10 good; content-view cut 3.8%, async cut 3.8%, symbols returned.
- `.sh` Shell: 7.6/10 good; content-view cut 0.4%, async cut 0.4%, symbols returned.
- `.clj` Clojure: 7.8/10 good; content-view cut 0.6%, async cut 17%, symbols not returned.

## Weakest Agent Understanding Scores

- `.vue` Vue: 7.9/10 good; syntax anchors 6.7/10, structure 8/10, output health 10/10, context budget 6/10, symbols 10/10.
- `.rst` reStructuredText: 8/10 strong; syntax anchors 6.7/10, structure 10/10, output health 10/10, context budget 6/10, symbols 7/10.
- `.pm` Perl Module: 8.1/10 strong; syntax anchors 6.7/10, structure 10/10, output health 10/10, context budget 7/10, symbols 7/10.
- `.pl` Perl: 8.2/10 strong; syntax anchors 6.7/10, structure 10/10, output health 10/10, context budget 8/10, symbols 7/10.
- `.clj` Clojure: 9.1/10 excellent; syntax anchors 10/10, structure 10/10, output health 9/10, context budget 6/10, symbols 7/10.
- `.json` JSON: 9.2/10 excellent; syntax anchors 10/10, structure 10/10, output health 10/10, context budget 5/10, symbols 7/10.
- `.jsonc` JSONC: 9.2/10 excellent; syntax anchors 10/10, structure 10/10, output health 10/10, context budget 5/10, symbols 7/10.
- `.md` Markdown: 9.2/10 excellent; syntax anchors 10/10, structure 10/10, output health 10/10, context budget 5/10, symbols 7/10.
- `.yml` YAML: 9.2/10 excellent; syntax anchors 10/10, structure 10/10, output health 9/10, context budget 7/10, symbols 7/10.
- `.ex` Elixir: 9.3/10 excellent; syntax anchors 10/10, structure 10/10, output health 10/10, context budget 6/10, symbols 7/10.

## Regenerate

```bash
yarn build
node benchmark/generate-real-code-report.mjs /path/to/real/corpus
```
