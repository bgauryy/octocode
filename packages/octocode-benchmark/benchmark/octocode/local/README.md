# Local tool benchmark questions

These rows exercise the local tool family and the local side of `octocode search`: ripgrep text search, structural AST search, file/content reads, directory and file discovery, LSP semantics, binary/archive inspection, and OQL local targets.

Use `LOCAL` as the monorepo root. Run all commands from the repository root, and capture every raw output under `$BENCH_OUT/raw`.

## Local Setup

Create the archive fixture only when artifact rows are included in the run:

```bash
mkdir -p .octocode/eval-fixtures/archive-src
printf '{"name":"octocode-eval-fixture"}\n' > .octocode/eval-fixtures/archive-src/package.json
printf 'export const fixture = "octocode-eval-fixture";\n' > .octocode/eval-fixtures/archive-src/index.ts
tar -czf .octocode/eval-fixtures/sample.tgz -C .octocode/eval-fixtures archive-src
```

For LSP rows, first run a symbol-discovery row and carry the returned line into position-anchored operations. Do not guess `lineHint`.

## Schema Questions

| ID | Surface | Question | Command | Pass criteria |
|---|---|---|---|---|
| LCL-SCHEMA-01 | quick-cli | Does top-level help expose search-first local commands, raw tools, and LSP line-hint guidance? | `octocode --help --no-color` | `search`, remaining quick commands (`clone`, `cache`, `unzip`), local raw tools, minification, and line-hint instructions are present; removed legacy shortcuts (`cat`, `ls`, `find`, `grep`, `history`, `repo`, `pkg`, `pr`, `lsp`, `binary`, `diff`) are absent as commands. |
| LCL-SCHEMA-02 | raw-tool | Do all local raw tool schemes document fields used below? | `octocode tools localSearchCode localGetFileContent localViewStructure localFindFiles localBinaryInspect lspGetSemantics oqlSearch --scheme --compact --no-color` | Required fields, pagination fields, minification modes, match fields, LSP ops, binary modes, and OQL local target fields are present. |
| LCL-SCHEMA-03 | search-cli | Does `octocode search` describe local shorthand and OQL routing? | `octocode search --scheme --compact --no-color` and `octocode search --help --no-color` | Active targets include `code`, `content`, `structure`, `files`, `semantics`, `artifacts`, `research`, and `graph`; help explains local file/dir routing and `--explain --dry-run`. |
| LCL-SCHEMA-04 | metadata | Does the offline metadata gate pass? | `NO_COLOR=1 node packages/octocode-benchmark/benchmark/cli/check-cli-metadata.mjs` | Reports 14 tools, 12 commands, and no stale schema/help/route failures. |

## Tool-By-Tool Questions

| ID | Tool / command | Question | Command or query | Pass criteria |
|---|---|---|---|---|
| LCL-SEARCH-01 | `localSearchCode` | Can discovery mode cheaply locate files before content reads? | `tools localSearchCode --queries '{"path":"packages/octocode-tools-core/src/oql","keywords":"target","mode":"discovery","itemsPerPage":10,"page":1}' --json --compact` | Returns paths only or compact rows, includes page context, no full snippets unless requested. |
| LCL-SEARCH-02 | `localSearchCode` | Can detailed ripgrep output preserve line, snippet, and context? | `tools localSearchCode --queries '{"path":"packages/octocode-tools-core/src/oql","keywords":"target","mode":"detailed","contextLines":2,"maxMatchesPerFile":5,"matchPage":1,"itemsPerPage":5}' --json --compact` | Each match has file/line/context; noisy files expose `matchPage` continuation without truncation. |
| LCL-SEARCH-03 | `localSearchCode` | Can regex and PCRE-style needs be diagnosed or routed honestly? | `tools localSearchCode --queries '{"path":"packages/octocode/src","keywords":"runOqlSearch\\(","perlRegex":false,"mode":"paginated","itemsPerPage":5}' --json --compact` | Regex result is non-empty or an explicit regex diagnostic explains the failure. |
| LCL-SEARCH-04 | `localSearchCode` | Can `onlyMatching` enumerate hits in compact or minified files? | `tools localSearchCode --queries '{"path":"packages/octocode/src/cli/commands/search.ts","keywords":"getString\\([^)]*\\)","onlyMatching":true,"unique":true,"matchWindow":12,"maxMatchesPerFile":10,"matchPage":1}' --json --compact` | Output contains matched substrings, optional context windows, uniqueness behavior, and match-page continuation if capped. |
| LCL-SEARCH-05 | `localSearchCode` | Can structural AST search find real call shapes and avoid string/comment false positives? | `tools localSearchCode --queries '{"path":"packages/octocode/src/cli/commands/search.ts","mode":"structural","pattern":"getString($$$ARGS)","langType":"ts","itemsPerPage":10}' --json --compact` | Matches are AST-backed with file/line/captures when available; zero matches include structural guidance, not a false absence claim. |
| LCL-SEARCH-06 | `octocode search` | Does search shorthand lower local text controls into OQL controls? | `octocode search "runOqlSearch" packages/octocode/src/cli/commands/search.ts --context-lines 2 --max-matches 3 --json --compact` | Normalized result uses target `code`, local source, text predicate, capped matches, and runnable content follow-up. |
| LCL-SEARCH-07 | `octocode search` | Does local structural shorthand require `--lang` and preserve AST pattern? | `octocode search --pattern 'getString($$$ARGS)' packages/octocode/src/cli/commands/search.ts --lang ts --json --compact` | Target is `code`; predicate is structural; missing `--lang` variant fails with usage guidance. |
| LCL-CONTENT-01 | `localGetFileContent` | Can search-to-fetch use `matchString` for exact local proof? | `tools localGetFileContent --queries '{"path":"packages/octocode/src/cli/commands/search.ts","matchString":"runOqlSearch","contextLines":4,"minify":"none"}' --json --compact` | Exact content includes the anchor, context, `matchRanges[]`, and no hidden truncation. |
| LCL-CONTENT-02 | `localGetFileContent` | Do minification modes differ on the same file? | `tools localGetFileContent --queries '[{"path":"packages/octocode/src/cli/commands/search.ts","matchString":"searchCommand","contextLines":2,"minify":"symbols"},{"path":"packages/octocode/src/cli/commands/search.ts","matchString":"searchCommand","contextLines":2,"minify":"standard"},{"path":"packages/octocode/src/cli/commands/search.ts","matchString":"searchCommand","contextLines":2,"minify":"none"}]' --json --compact` | `symbols`, `standard`, and `none` are visibly distinct and all preserve usable line anchors. |
| LCL-CONTENT-03 | `localGetFileContent` | Can char pagination continue a large local file without losing filters? | `tools localGetFileContent --queries '{"path":"packages/octocode/src/cli/commands/search.ts","charOffset":0,"charLength":4000,"minify":"none"}' --json --compact` then follow `nextCharOffset`. | Page 2 is a different window; `hasMore` and `nextCharOffset` are concrete. |
| LCL-CONTENT-04 | `octocode search` | Does content shorthand route a file path to target `content`? | `octocode search packages/octocode/src/cli/commands/search.ts --content-view symbols --json --compact` | Result is a content view, not a code search; `contentView:"symbols"` is preserved. |
| LCL-STRUCTURE-01 | `localViewStructure` | Can local tree browsing page with details? | `tools localViewStructure --queries '{"path":"packages/octocode-tools-core/src/oql","details":true,"itemsPerPage":10,"page":1}' --json --compact` | Entries include path/type/details and page continuation when more entries exist. |
| LCL-STRUCTURE-02 | `octocode search` | Does `search <dir> --tree` route to structure? | `octocode search packages/octocode-tools-core/src/oql --tree --depth 2 --json --compact` | Target is structure; depth and path are preserved; no file bodies are read. |
| LCL-FILES-01 | `localFindFiles` | Can filename and metadata discovery find manifests without reading content? | `tools localFindFiles --queries '{"path":".","names":["package.json"],"entryType":"f","details":true,"itemsPerPage":10,"page":1}' --json --compact` | Rows are file paths/metadata only; pagination and sort are explicit. |
| LCL-FILES-02 | `octocode search` | Does OQL files target preserve field predicates? | `octocode search package.json . --target files --name package.json --details --json --compact` | Normalized query is target `files`; output has file rows, not snippets. |
| LCL-LSP-01 | `lspGetSemantics` | Can document symbols outline a TypeScript file? | `tools lspGetSemantics --queries '{"uri":"packages/octocode/src/cli/commands/search.ts","type":"documentSymbols","itemsPerPage":25}' --json --compact` | Symbol rows include names/kinds/ranges or an honest capability diagnostic. |
| LCL-LSP-02 | `lspGetSemantics` | Can a search/content anchor feed references without guessed lines? | Run LCL-LSP-01, select `searchCommand` line, then `tools lspGetSemantics --queries '{"uri":"packages/octocode/src/cli/commands/search.ts","type":"references","symbolName":"searchCommand","lineHint":<line>,"groupByFile":true}' --json --compact`. | Uses the discovered line; references are grouped or explicitly unavailable. |
| LCL-LSP-03 | `octocode search` | Does search CLI semantics require/encourage exact line anchors? | `octocode search packages/octocode/src/cli/commands/search.ts --op documentSymbols --json --compact`, then `--op references --symbol searchCommand --line <line>`. | Help/schema line-hint warning matches behavior; wrong or missing line produces a useful diagnostic. |
| LCL-BINARY-01 | `localBinaryInspect` | Can native binary inspect report format metadata? | `tools localBinaryInspect --queries '{"path":"packages/octocode-engine/npm/darwin-arm64/octocode-engine.darwin-arm64.node","mode":"inspect","detailed":false}' --json --compact` | Returns format/arch/counts/deps or an honest missing fixture/platform diagnostic. |
| LCL-BINARY-02 | `localBinaryInspect` | Can archive listing page entries before extraction? | `tools localBinaryInspect --queries '{"path":".octocode/eval-fixtures/sample.tgz","mode":"list","entriesPerPage":5,"entryPageNumber":1,"verbose":true}' --json --compact` | Entries include `archive-src/package.json`; archive page fields are present. |
| LCL-BINARY-03 | `localBinaryInspect` | Can archive extraction use exact listed entry names and content paging? | `tools localBinaryInspect --queries '{"path":".octocode/eval-fixtures/sample.tgz","mode":"extract","archiveFile":"archive-src/package.json","charLength":2000}' --json --compact` | Extracted content is exact or paginated with `nextCharOffset`; entry path came from LCL-BINARY-02. |
| LCL-BINARY-04 | `localBinaryInspect` | Can unpack output become a normal local corpus? | `tools localBinaryInspect --queries '{"path":".octocode/eval-fixtures/sample.tgz","mode":"unpack"}' --json --compact`, then run `search` on returned `localPath`. | `localPath` is usable by search/OQL local targets and marked as generated artifact output. |
| LCL-BINARY-05 | `localBinaryInspect` | Can strings mode page by scan offset? | `tools localBinaryInspect --queries '{"path":"packages/octocode-engine/npm/darwin-arm64/octocode-engine.darwin-arm64.node","mode":"strings","minLength":12,"includeOffsets":true,"scanOffset":0}' --json --compact` | Strings include offsets and `nextScanOffset` when partial; no unbounded dump. |
| LCL-OQL-01 | `oqlSearch` | Does raw OQL local code preserve evidence and continuations? | `tools oqlSearch --queries '{"target":"code","from":{"kind":"local","path":"packages/octocode/src/cli/commands"},"where":{"kind":"text","value":"runOqlSearch"},"view":"paginated","itemsPerPage":5}' --json --compact` | Row has local source/path/line/snippet, evidence fields, and content continuation. |
| LCL-OQL-02 | `octocode search` | Does `--explain --dry-run` show local route decisions before execution? | `octocode search --explain --dry-run --query '{"target":"code","from":{"kind":"local","path":"packages/octocode/src/cli/commands"},"where":{"kind":"text","value":"runOqlSearch"}}' --json` | Plan includes normalized query, backend route, transformer id, and no execution data. |
| LCL-OQL-03 | `research` | Can research page 1 return summary without bulk payload? | `octocode search --query '{"target":"research","from":{"kind":"local","path":"packages/octocode-tools-core/src/oql"},"params":{"intent":"symbols","facets":["symbols","files"],"maxFiles":20},"itemsPerPage":1,"page":1}' --json --compact` | `data.summary` is present; evidence is candidate-grade; `answerReady:false` is expected and explained. |
| LCL-OQL-04 | `graph` | Can graph upgrade a research packet toward LSP proof? | Follow `next.graph` from LCL-OQL-03 or run a bounded `target:"graph"` query with `proof:"lsp","proofLimit":5`. | Rows include `proofStatus` or a capability diagnostic; candidate rows are not mislabeled as proof. |

## Local Reflection Questions

| Prompt | What to record |
|---|---|
| Issues | Unexpected exit codes, missing localPath, missing `matchRanges[]`, bad lineHint guidance, structural zero-match ambiguity, binary fixture gaps. |
| Improvements | Product reason and benchmark reason, especially around match paging, structural hints, LSP capability diagnostics, and archive/string pagination. |
| Good flow | The shortest local chain that got from cheap orientation to exact proof with the lowest output tokens. |
| Instruction gaps | Missing, contradictory, stale, or non-working instructions in `--help`, `search --scheme`, raw tool schemes, or agent guidance. |
