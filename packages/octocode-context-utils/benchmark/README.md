# Real-Code Minifier Benchmark

This directory records before/after excerpts and metrics for one real sample per
discovered extension. Full third-party source files are not vendored here; use
the generator to recreate reports from a local corpus.

## Summary

- Samples covered: 46
- Symbol skeletons returned: 32/32
- Average cuts: content-view 27.7%, apply 31.8%, async 31.8%

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

| Ext | Format | Configured strategy | Async type | Input bytes | Content-view cut | Apply cut | Sync cut | Async cut | Symbols cut | Source |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `js` | JavaScript | `terser` | `terser` | 6864 | 9.6% | 20.9% | 20.9% | 20.9% | 16% | `js/00-react-hooks.js` |
| `cjs` | CommonJS | `terser` | `terser` | 3184 | 49.6% | 49.6% | 49.6% | 49.6% | -19.1% | `cjs/apidom-babel.config.cjs` |
| `mjs` | ESM JavaScript | `terser` | `terser` | 1259 | 31.2% | 31.2% | 31.2% | 31.2% | -14.6% | `mjs/llhttp-eslint.config.mjs` |
| `jsx` | JSX | `terser` | `terser` | 3825 | 25.2% | 25.2% | 25.2% | 25.2% | 84.3% | `jsx/00-fullcalendar-demo.jsx` |
| `ts` | TypeScript | `conservative` | `conservative` | 92419 | 59.6% | 59.6% | 59.6% | 59.6% | 69.2% | `ts/00-typescript-core.ts` |
| `tsx` | TSX | `conservative` | `conservative` | 23197 | 50.2% | 50.2% | 50.2% | 50.2% | 83.9% | `tsx/00-next-app-router.tsx` |
| `json` | JSON | `json` | `json` | 3468 | 0% | 29% | 29% | 29% | n/a | `json/typescript-package.json` |
| `jsonc` | JSONC | `json` | `json` | 1427 | 0% | 15.2% | 15.2% | 15.2% | n/a | `jsonc/grammy-deno.jsonc` |
| `css` | CSS | `aggressive` | `aggressive` | 280311 | 18.1% | 18.1% | 18.1% | 18.1% | 70.7% | `css/bootstrap.css` |
| `scss` | SCSS | `aggressive` | `aggressive` | 7057 | 23.3% | 23.3% | 23.3% | 23.3% | 78.4% | `scss/_buttons.scss` |
| `html` | HTML | `aggressive` | `aggressive` | 5096 | 0% | 13.5% | 13.5% | 13.5% | 95.4% | `html/00-mdn-letter.html` |
| `vue` | Vue | `aggressive` | `aggressive` | 119 | 0.8% | 6.7% | 6.7% | 6.7% | 26.9% | `vue/vite-app.vue` |
| `svelte` | Svelte | `aggressive` | `aggressive` | 2665 | 0% | 21.4% | 21.4% | 21.4% | 87.1% | `svelte/vite-app.svelte` |
| `py` | Python | `conservative` | `conservative` | 65713 | 21.3% | 21.2% | 21.2% | 21.2% | 53.3% | `py/00-httpx-client.py` |
| `java` | Java | `conservative` | `conservative` | 63265 | 64.8% | 64.7% | 64.7% | 64.7% | 87.3% | `java/00-spring-annotation-utils.java` |
| `go` | Go | `conservative` | `conservative` | 33315 | 34.1% | 33.9% | 33.9% | 33.9% | 86.8% | `go/print.go` |
| `rs` | Rust | `conservative` | `conservative` | 100057 | 62.2% | 62.1% | 62.1% | 62.1% | 66.1% | `rs/option.rs` |
| `c` | C | `conservative` | `conservative` | 18107 | 3.8% | 3.8% | 3.8% | 3.8% | 68.5% | `c/00-git-add.c` |
| `cpp` | C++ | `conservative` | `conservative` | 32621 | 30.2% | 30.1% | 30.1% | 30.1% | 76.4% | `cpp/00-llvm-raw-ostream.cpp` |
| `h` | C Header | `conservative` | `conservative` | 33059 | 39% | 38.9% | 38.9% | 38.9% | 41.1% | `h/git-compat-util.h` |
| `hpp` | C++ Header | `conservative` | `conservative` | 25322 | 38.3% | 38.2% | 38.2% | 38.2% | 39.3% | `hpp/fmt-color.hpp` |
| `cs` | C# | `conservative` | `conservative` | 5603 | 28.3% | 28.2% | 28.2% | 28.2% | 52.7% | `cs/00-dotnet-argument-exception.cs` |
| `php` | PHP | `conservative` | `conservative` | 35469 | 41.4% | 41.2% | 41.2% | 41.2% | 87.1% | `php/Arr.php` |
| `rb` | Ruby | `conservative` | `conservative` | 3507 | 64.2% | 63.8% | 63.8% | 63.8% | 81.5% | `rb/blank.rb` |
| `sh` | Shell | `conservative` | `conservative` | 156857 | 0.4% | 0.3% | 0.3% | 0.3% | 97.3% | `sh/nvm.sh` |
| `sql` | SQL | `conservative` | `conservative` | 8415 | 35.6% | 35.3% | 35.3% | 35.3% | 94.6% | `sql/00-postgres-select.sql` |
| `yml` | YAML | `conservative` | `conservative` | 12508 | 6.2% | 6.2% | 6.2% | 6.2% | n/a | `yaml/typescript-ci.yml` |
| `toml` | TOML | `conservative` | `conservative` | 3039 | 38.1% | 38% | 38% | 38% | n/a | `toml/rust-cargo.toml` |
| `lua` | Lua | `aggressive` | `aggressive` | 23250 | 15.6% | 27.8% | 27.8% | 27.8% | -11.7% | `lua/plenary-path.lua` |
| `graphql` | GraphQL | `conservative` | `conservative` | 1300 | 3.2% | 3.1% | 3.1% | 3.1% | 35.4% | `graphql/graphql-go-kitchen-sink.graphql` |
| `md` | Markdown | `markdown` | `markdown` | 3304 | 1.2% | 1.2% | 1.2% | 1.2% | 40.6% | `md/rust-readme.md` |
| `rst` | reStructuredText | `conservative` | `conservative` | 2616 | 1.8% | 1.7% | 1.7% | 1.7% | n/a | `rst/cpython-tutorial-index.rst` |
| `scala` | Scala | `conservative` | `conservative` | 20107 | 80.7% | 80.5% | 80.5% | 80.5% | 94.1% | `scala/Option.scala` |
| `swift` | Swift | `conservative` | `conservative` | 33805 | 65.5% | 65.4% | 65.4% | 65.4% | 81.3% | `swift/Optional.swift` |
| `kt` | Kotlin | `conservative` | `conservative` | 20559 | 49.1% | 49% | 49% | 49% | 66.4% | `kt/Collections.kt` |
| `dart` | Dart | `conservative` | `conservative` | 37049 | 85.5% | 85.3% | 85.3% | 85.3% | 98.8% | `dart/dart-string.dart` |
| `r` | R | `aggressive` | `aggressive` | 15796 | 46.6% | 57.9% | 57.9% | 57.9% | 90.3% | `r/dplyr-mutate.R` |
| `proto` | Protocol Buffers | `conservative` | `conservative` | 60347 | 69.1% | 68.8% | 68.8% | 68.8% | 95.2% | `proto/protobuf-descriptor.proto` |

## Regenerate

```bash
yarn build
node benchmark/generate-real-code-report.mjs /path/to/real/corpus
```
