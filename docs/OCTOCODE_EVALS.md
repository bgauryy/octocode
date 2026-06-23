# Octocode Unified Eval

This is the single canonical eval for Octocode CLI commands, raw tools, and
OQL (`octocode search`). It is a measurement suite, not a run log: keep local
machine notes and one-off observations out of this file.

Use this doc to answer one question: can an agent start from a research
question, choose the right surface, get trustworthy data, page/fetch deeper, and
stay token-efficient across local, GitHub, npm, LSP, binary, archive, and
cross-repository flows?

## Scope

| Surface | What must be measured |
|---|---|
| CLI quick commands | `ls`, `cat`, `grep`, `find`, `repo`, `pr`, `history`, `pkg`, `binary`, `unzip`, `clone`, `tools`, `context`, help, JSON envelopes |
| Raw tools | All 14 shared tool runners through `tools <name> --queries` |
| OQL | All active `octocode search` targets and the OQL-to-tool transformations that make them work |
| GitHub research | Repo search, code search, file/path search, structure, content fetch, PR list/detail/comments/reviews/commits, commit history, clone/materialize |
| Local research | Text/regex search, structural search, file finding, structure, content ranges, minification, LSP semantics |
| Packages | npm package lookup and package-to-source handoff |
| Binary/archive | Native binary inspect/strings, archive list/extract/unpack, and local research after `unzip` |
| Cross-repo flows | Compare related implementations across LangChain, LangGraph, Zustand, OpenClaw candidates, and Hermes repos |

## Corpus

Use stable, research-rich repos. Do not substitute a large generic framework
repo just because it is familiar.

| Alias | Corpus | Why it is in the eval |
|---|---|---|
| `LCJS` | `langchain-ai/langchainjs` | TypeScript code search, PR archaeology, streaming APIs, package structure |
| `LCPY` | `langchain-ai/langchain` | Python comparison target and large-repo pagination |
| `LGJS` | `langchain-ai/langgraphjs` | Cross-repo comparison with LangChain.js and docs/examples split |
| `LGPY` | `langchain-ai/langgraph` | Python graph/runtime comparison and large tree structure |
| `ZUSTAND` | `pmndrs/zustand` | Small but real TypeScript package, npm-to-repo handoff, minification checks |
| `HERMES_AGENT` | `NousResearch/hermes-agent` | Agent/tooling repo with Python, package files, and web assets |
| `HERMES_ENGINE` | `facebook/hermes` | Native/runtime repo for structure, CMake/content, and non-TS code |
| `OPENCLAW` | `Gen-Verse/OpenClaw-RL` plus candidates from repo search | Discovery-quality and fallback behavior; deep checks must first prove the selected repo is browseable |
| `LOCAL` | This monorepo root | Local search, structural search, LSP, binary/archive fixtures, and dogfooding |

## Scoring

Each row is scored independently.

| Score | Meaning |
|---:|---|
| `2` | Pass: correct data, usable continuations, honest diagnostics, and the answer can be followed up without guessing |
| `1` | Partial: data is present but one required field, page, diagnostic, or follow-up is missing or ambiguous |
| `0` | Fail: empty or wrong result, silent lossy normalization, bad routing, unusable pagination, or false proof of absence |
| `N/A` | Environment gated: auth, clone, LSP server, or artifact fixture is unavailable and the tool says so honestly |

Surface-level pass requires:

- Data quality: rows include enough source context to identify repo/path/file/line/symbol/PR/commit/package/artifact.
- Searchability: shorthand and full JSON forms route to the same backing capability for equivalent questions.
- Fetchability: search rows that claim to be followable expose an executable `next.fetch`, `next.page`, `next.matchPage`, `next.charRange`, `next.structure`, or equivalent raw-tool continuation.
- Pagination: `hasMore` implies a concrete next query; page 2 returns a different window without losing filters.
- Minification: exact, compact/standard, and symbols views behave distinctly and never hide that content is partial.
- Token efficiency: triage commands use concise/discovery/compact output before detailed content or full PR/diff reads.
- Error honesty: empty GitHub/provider results are not treated as proof of absence unless materialized/local proof was requested and completed.
- OQL parity: OQL may normalize rows, but it must preserve the context needed to continue the same investigation.
- OQL proof grading: every OQL result row must include `proofGrade` as one of `candidate`, `text`, `structural`, `semantic`, `graph`, or `missing`.

## Setup

Run from the repository root after a fresh build:

```bash
yarn build
node packages/octocode/out/octocode.js --help --no-color
```

For archive rows, create or provide an archive fixture. A minimal fixture is
enough; the value being tested is archive handling and the follow-up local flow.

```bash
mkdir -p .octocode/eval-fixtures/archive-src
printf '{"name":"octocode-eval-fixture"}\n' > .octocode/eval-fixtures/archive-src/package.json
printf 'export const fixture = "octocode-eval-fixture";\n' > .octocode/eval-fixtures/archive-src/index.ts
tar -czf .octocode/eval-fixtures/sample.tgz -C .octocode/eval-fixtures archive-src
```

Use `--json` for machine scoring and `--compact` for token-budget checks where
the command supports it.

## OQL Transformations Under Test

OQL is a typed query layer that compiles research intent into vendor-specific
tool calls. These transformations are part of the eval:

| OQL input | Expected backing behavior |
|---|---|
| Shorthand `search "x" owner/repo --type ts` | GitHub code search scoped to TypeScript/`.ts` instead of an empty local-only language alias |
| `target:"code"` with `from.kind:"github"` | `ghSearchCode` provider search when possible; materialized local search only when requested or required |
| `target:"files"` on GitHub | GitHub path search, not content snippets, when the predicate is path-like |
| `target:"content"` with `fetch.content.match` | `ghGetFileContent` or `localGetFileContent` match-anchored read with context |
| `contentView:"exact"|"compact"|"symbols"` | Raw tool minification `none|standard|symbols` as appropriate for the backing tool |
| `target:"pullRequests"|"commits"|"diff"` | `ghHistoryResearch` with the correct PR, commit, or patch mode; `diff` also supports direct two-ref file diffs via content reads |
| `target:"repositories"` | `ghSearchRepos` with language/topic/star/page filters preserved |
| `target:"packages"` | `npmSearch`, then source repo fields remain available for handoff |
| `target:"artifacts"` | `localBinaryInspect` modes: inspect/list/extract/decompress/strings/unpack |
| `target:"semantics"` | `lspGetSemantics`; remote semantics require materialization before LSP proof |
| `target:"materialize"` | `ghCloneRepo`/cache checkpoint with local follow-up paths |
| `target:"research"|"graph"` | Smart local/materialized corpus analysis with packet pagination and candidate/proof diagnostics |

Transformer honesty rule: an OQL transformer that drops, narrows, or weakens a
user-supplied predicate/scope must emit a blocking diagnostic such as
`lossyTransform`, `unsupportedVendorPredicate`, `vendorNoEquivalent`, or
`responseShapeMismatch`. A silent lossy mapping is an eval failure even when the
returned rows look plausible.

## CLI Command Rows

| ID | Question | Command | Measure |
|---|---|---|---|
| CLI-01 | Does top-level help expose the command surface and global flags? | `node packages/octocode/out/octocode.js --help --no-color` | Commands are discoverable; no stale command names |
| CLI-02 | Does the agent context expose current tool guidance? | `node packages/octocode/out/octocode.js context --full --no-color` | Includes tool guidance and command guidance without implementation drift |
| CLI-03 | Can raw tools be listed? | `node packages/octocode/out/octocode.js tools --no-color` | All 14 tools are present |
| CLI-04 | Can an agent read a schema before calling a raw tool? | `node packages/octocode/out/octocode.js tools ghSearchCode --scheme --compact --no-color` | Field names, defaults, pagination, and output envelope are documented |
| CLI-05 | Can repo discovery identify the corpus candidates? | `node packages/octocode/out/octocode.js repo langchain --json --compact` | Returns `LCJS`/`LCPY` candidates with owner/repo, stars, language |
| CLI-06 | Can a remote structure be browsed cheaply? | `node packages/octocode/out/octocode.js ls langchain-ai/langgraphjs/libs --depth 2 --json --compact` | Directory rows page correctly and preserve paths |
| CLI-07 | Can remote content be read in a cheap symbol/minified view? | `node packages/octocode/out/octocode.js cat pmndrs/zustand/src/vanilla.ts --mode symbols --json --compact` | Output is smaller than exact text and keeps line anchors |
| CLI-08 | Does `--type ts` find TypeScript GitHub code through shorthand? | `node packages/octocode/out/octocode.js grep "_streamChatModelEvents" langchain-ai/langchainjs --type ts --json --compact` | Non-empty results; top rows include path, snippets, match offsets |
| CLI-09 | Can path search find package manifests without reading content? | `node packages/octocode/out/octocode.js find "package.json" pmndrs/zustand --source github --search path --json --compact` | Uses path search; no snippet-only rows required |
| CLI-10 | Can PR list mode find a focused candidate set? | `node packages/octocode/out/octocode.js pr langchain-ai/langchainjs --query "_streamChatModelEvents" --state merged --limit 5 --json` | PR rows include number/title/state/date and pagination |
| CLI-11 | Can PR detail mode deep-read PR context, comments, commits, reviews, and patches? | `node packages/octocode/out/octocode.js pr langchain-ai/langchainjs#10924 --deep --match-string "_streamChatModelEvents" --json` | Detail sections are present or honestly paginated |
| CLI-12 | Can commit history be scoped to a path? | `node packages/octocode/out/octocode.js history langchain-ai/langchainjs/libs/langchain-core --limit 5 --json` | Commits include sha/date/author/message and next page if available |
| CLI-13 | Can npm resolve a package to source? | `node packages/octocode/out/octocode.js pkg zustand --json` | Package version and repository URL are present |
| CLI-14 | Can an archive be listed before unpacking? | `node packages/octocode/out/octocode.js binary .octocode/eval-fixtures/sample.tgz --list --json` | Entries include `archive-src/package.json` and pagination fields |
| CLI-15 | Can an archive be unpacked and then researched locally? | `node packages/octocode/out/octocode.js unzip .octocode/eval-fixtures/sample.tgz --json` | Returns `localPath`; subsequent `ls`/`find`/`grep`/`cat` work on that path |
| CLI-16 | Can clone-backed remote search switch to local tools? | `node packages/octocode/out/octocode.js grep --repo pmndrs/zustand "createStore" src --type ts --json` | Response includes saved local location and local search rows |

## Raw Tool Rows

Run every row through `node packages/octocode/out/octocode.js tools <tool> --queries '<json>' --json --compact`.

| ID | Tool | Question | Query | Measure |
|---|---|---|---|---|
| TOOL-01 | `ghSearchRepos` | Can repo search discover all benchmark families? | `[{"keywords":["langchain"],"limit":5},{"keywords":["langgraph"],"owner":"langchain-ai","limit":5},{"keywords":["zustand"],"limit":5},{"keywords":["open claw"],"limit":10},{"keywords":["hermes"],"limit":10}]` | Bulk query returns 5 result groups; OpenClaw is treated as candidate discovery |
| TOOL-02 | `ghSearchCode` | Does content search preserve snippet offsets? | `{"keywords":["streamEvents"],"owner":"langchain-ai","repo":"langchainjs","extension":"ts","match":"file","limit":5,"concise":false}` | Rows include owner, repo, path, queryId, snippet, and match indices |
| TOOL-03 | `ghSearchCode` | Does path search stay cheap? | `{"keywords":["package.json"],"owner":"pmndrs","repo":"zustand","match":"path","limit":10}` | Rows include paths and no unnecessary content snippets |
| TOOL-04 | `ghGetFileContent` | Can GitHub content be read by exact range and match anchor? | `{"owner":"pmndrs","repo":"zustand","path":"src/vanilla.ts","matchString":"createStore","contextLines":4,"minify":"none"}` | Exact text includes context and `matchRanges` line anchors |
| TOOL-05 | `ghGetFileContent` | Do GitHub minification modes differ? | `[{"owner":"pmndrs","repo":"zustand","path":"src/vanilla.ts","fullContent":true,"minify":"symbols"},{"owner":"pmndrs","repo":"zustand","path":"src/vanilla.ts","fullContent":true,"minify":"standard"}]` | Symbols is skeletal; standard is compact readable content |
| TOOL-06 | `ghViewRepoStructure` | Can a remote tree page and continue? | `{"owner":"langchain-ai","repo":"langgraphjs","path":"libs","maxDepth":2,"itemsPerPage":10,"page":1}` | Structure rows preserve paths; `hasMore` carries next page |
| TOOL-07 | `ghHistoryResearch` | Can PR list search find relevant discussions? | `{"type":"prs","owner":"langchain-ai","repo":"langchainjs","keywordsToSearch":["streamEvents"],"state":"merged","limit":5,"page":1}` | PR rows include number/title/state and page info |
| TOOL-08 | `ghHistoryResearch` | Can PR detail deep-read a known PR? | `{"type":"prs","owner":"langchain-ai","repo":"langchainjs","prNumber":10924,"reviewMode":"full","matchString":"_streamChatModelEvents","itemsPerPage":20}` | Header fields, body, changed files, comments, reviews, commits, and content pagination are preserved |
| TOOL-09 | `ghHistoryResearch` | Can commit history for a path be read? | `{"type":"commits","owner":"langchain-ai","repo":"langchainjs","path":"libs/langchain-core","perPage":5,"page":1}` | Commit rows include sha/date/author/message and next page |
| TOOL-10 | `ghCloneRepo` | Can a bounded sparse clone become a local corpus? | `{"owner":"pmndrs","repo":"zustand","sparsePath":"src"}` | Returns local path/location for follow-up local tools |
| TOOL-11 | `npmSearch` | Can npm resolve package-to-source handoff? | `{"packageName":"zustand","mode":"full"}` | Package version, repository URL, and package fields are present |
| TOOL-12 | `localViewStructure` | Can local tree browsing page with details? | `{"path":"packages/octocode-tools-core/src/oql","details":true,"itemsPerPage":10,"page":1}` | Entries include names/types/sizes and continuation when needed |
| TOOL-13 | `localFindFiles` | Can local file search find manifests and file attributes? | `{"path":".","names":["package.json"],"entryType":"f","details":true,"itemsPerPage":10}` | Paths are workspace-relative or absolute and sortable |
| TOOL-14 | `localSearchCode` | Can text, regex, only-matching, and match paging work locally? | `{"path":"packages/octocode-tools-core/src/oql","keywords":"target","mode":"detailed","maxMatchesPerFile":5,"matchPage":1,"contextLines":2}` | Match rows include context, per-file pagination, and no silent truncation |
| TOOL-15 | `localSearchCode` | Can structural search avoid string/comment false positives? | `{"path":"packages/octocode-tools-core/src/oql","mode":"structural","pattern":"diagnostic($$$ARGS)","langType":"ts"}` | AST matches include file/line and structural mode diagnostics |
| TOOL-16 | `localGetFileContent` | Can local content read exact, compact, symbols, range, and match views? | `{"path":"packages/octocode-tools-core/src/oql/run.ts","matchString":"runOqlSearch","contextLines":4,"minify":"none"}` | Match range anchors are usable by LSP follow-up |
| TOOL-17 | `localBinaryInspect` | Can binary/archive modes run and continue? | `{"path":".octocode/eval-fixtures/sample.tgz","mode":"list","entriesPerPage":10,"entryPageNumber":1}` | Archive entries, page info, and extract/unpack next steps are clear |
| TOOL-18 | `lspGetSemantics` | Can LSP outline and semantic navigation use real anchors? | `{"uri":"packages/octocode-tools-core/src/oql/run.ts","type":"documentSymbols","itemsPerPage":25}` | Symbols include names/kinds/ranges; unsupported servers report capability absence, not no symbols |
| TOOL-19 | `oqlSearch` | Does the MCP-style OQL tool preserve the same typed envelope as `octocode search`? | `{"target":"code","from":{"kind":"local","path":"packages/octocode-tools-core/src/oql"},"where":{"kind":"structural","lang":"ts","pattern":"diagnostic($$$ARGS)"},"limit":1}` | Returns OQL rows with `proofGrade`, captures/ranges, diagnostics, pagination, and executable `next.*` continuations |

## OQL Rows

Run with `node packages/octocode/out/octocode.js search --query '<json>' --json --compact` unless the row intentionally uses shorthand.

| ID | Target | Question | Command or query | Measure |
|---|---|---|---|---|
| OQL-01 | Schema | Does OQL expose all active targets? | `node packages/octocode/out/octocode.js search --scheme --compact --no-color` | Active targets include code, content, structure, files, semantics, repositories, packages, pullRequests, commits, artifacts, diff, research, graph, materialize |
| OQL-02 | Explain | Does planning show routing before execution? | `node packages/octocode/out/octocode.js search --explain --dry-run --query '{"target":"code","from":{"kind":"github","repo":"langchain-ai/langchainjs"},"where":{"kind":"regex","value":"streamEvents\\(","dialect":"pcre2"},"scope":{"language":"ts"},"materialize":{"mode":"auto"}}' --json` | Plan marks provider/residual/materialization decisions and does not execute |
| OQL-03 | Code shorthand | Does `--type ts` transform correctly for GitHub? | `node packages/octocode/out/octocode.js search "_streamChatModelEvents" langchain-ai/langchainjs --type ts --json --compact` | Non-empty TypeScript hits; no empty result caused by a local-only type alias |
| OQL-04 | Code full JSON | Do GitHub code rows preserve follow-up context? | `{"target":"code","from":{"kind":"github","repo":"langchain-ai/langchainjs"},"where":{"kind":"text","value":"streamEvents"},"scope":{"language":"ts"},"limit":5}` | Rows include path/source/snippet/match offsets and executable `next.fetch` |
| OQL-05 | Files | Can OQL map path-like predicates to GitHub file search? | `{"target":"files","from":{"kind":"github","repo":"pmndrs/zustand"},"where":{"kind":"field","field":"basename","op":"=","value":"package.json"},"limit":10}` | Returns package manifest paths without requiring content snippets |
| OQL-06 | Structure | Can remote structure page with the same scope intact? | `{"target":"structure","from":{"kind":"github","repo":"langchain-ai/langgraphjs"},"scope":{"path":"libs"},"fetch":{"tree":{"maxDepth":2}},"itemsPerPage":10,"page":1}` | `next.page` preserves repo/path/depth/page size |
| OQL-07 | Content | Can GitHub content fetch by match string? | `{"target":"content","from":{"kind":"github","repo":"pmndrs/zustand"},"scope":{"path":"src/vanilla.ts"},"fetch":{"content":{"match":{"text":"createStore"},"range":{"contextLines":4},"contentView":"exact"}}}` | Exact region includes the match and line anchors |
| OQL-08 | Minification | Do content views map to raw minification modes? | `{"queries":[{"target":"content","from":{"kind":"github","repo":"pmndrs/zustand"},"scope":{"path":"src/vanilla.ts"},"fetch":{"content":{"contentView":"symbols"}}},{"target":"content","from":{"kind":"github","repo":"pmndrs/zustand"},"scope":{"path":"src/vanilla.ts"},"fetch":{"content":{"contentView":"compact"}}},{"target":"content","from":{"kind":"github","repo":"pmndrs/zustand"},"scope":{"path":"src/vanilla.ts"},"fetch":{"content":{"contentView":"exact","range":{"startLine":1,"endLine":80}}}}]}` | Three views are distinct and partial content is explicit |
| OQL-09 | Repositories | Can repo discovery cover all benchmark families? | `{"queries":[{"target":"repositories","params":{"keywords":["langchain"],"limit":5}},{"target":"repositories","params":{"keywords":["langgraph"],"owner":"langchain-ai","limit":5}},{"target":"repositories","params":{"keywords":["zustand"],"limit":5}},{"target":"repositories","params":{"keywords":["open claw"],"limit":10}},{"target":"repositories","params":{"keywords":["hermes"],"limit":10}}]}` | Rows identify primary candidates and expose enough context for structure/content follow-up |
| OQL-10 | Packages | Can package lookup hand off to source repo research? | `{"target":"packages","params":{"packageName":"zustand","mode":"full"}}` | Repository URL/source fields are present and can feed GitHub rows |
| OQL-11 | Pull requests | Can PR search find candidate discussions? | `{"target":"pullRequests","from":{"kind":"github","repo":"langchain-ai/langchainjs"},"params":{"keywordsToSearch":["streamEvents"],"state":"merged","limit":5,"page":1}}` | PR rows include number/title/state and page continuation |
| OQL-12 | PR deep dive | Can one PR preserve comments, reviews, commits, files, and body? | `{"target":"pullRequests","from":{"kind":"github","repo":"langchain-ai/langchainjs"},"params":{"prNumber":10924,"reviewMode":"full","matchString":"_streamChatModelEvents","itemsPerPage":20}}` | Detail content is complete or honestly paginated; comments are not dropped |
| OQL-13 | Commits | Can commit mode preserve path and pagination context? | `{"target":"commits","from":{"kind":"github","repo":"langchain-ai/langchainjs"},"params":{"path":"libs/langchain-core","limit":5,"page":1}}` | Commit rows include parent repo/path context and `next.page` when present |
| OQL-14 | Diff | Can a PR diff be selected without reading unrelated content? | `{"target":"diff","from":{"kind":"github","repo":"langchain-ai/langchainjs"},"params":{"prNumber":10924,"files":["libs/langchain-core/src/language_models/chat_models.ts"]}}` | Diff rows are scoped, minified by default, and can page by content offset if needed |
| OQL-15 | Materialize | Can a bounded GitHub subtree become a local checkpoint? | `{"target":"materialize","from":{"kind":"github","repo":"pmndrs/zustand"},"scope":{"path":"src"},"materialize":{"mode":"required"}}` | Returns localPath/repoRoot/ref/cache completeness plus `next.structure` and `next.files` |
| OQL-16 | Semantics | Can materialized/local code feed LSP? | `{"target":"semantics","from":{"kind":"local","path":"packages/octocode-tools-core/src/oql/run.ts"},"params":{"type":"documentSymbols","itemsPerPage":25}}` | Symbols include ranges; semantic failures are capability diagnostics |
| OQL-17 | Research | Can smart packets summarize a bounded corpus cheaply? | `{"target":"research","from":{"kind":"local","path":"packages/octocode-tools-core/src/oql"},"params":{"mode":"analyze","intent":"symbols","facets":["symbols","files"],"maxFiles":20},"itemsPerPage":3}` | Packets are paged, cite files/symbols, and remain candidate-grade unless proof continuations are followed |
| OQL-18 | Graph | Can graph output expose relationships, not just snippets? | `{"target":"graph","from":{"kind":"local","path":"packages/octocode-tools-core/src/oql"},"params":{"mode":"analyze","intent":"dependencies","facets":["symbols","files","relations"],"maxFiles":20,"includePackets":true,"includeEdges":true},"itemsPerPage":3}` | Edges/facts cite packet/file evidence and page correctly |
| OQL-19 | Artifacts | Can artifact targets map to archive modes? | `{"target":"artifacts","from":{"kind":"local","path":".octocode/eval-fixtures/sample.tgz"},"params":{"mode":"list","entryPageNumber":1}}` | Archive rows match raw `localBinaryInspect` list mode |
| OQL-20 | Reserved | Do unsupported targets fail cleanly? | `{"target":"dataflow","from":{"kind":"local","path":"."}}` | Returns an explicit unsupported-target diagnostic, not silent fallback |
| OQL-21 | Tool parity | Do `octocode search --query` and `tools oqlSearch --queries` agree for the same OQL object? | Run the TOOL-19 query once through `search --query` and once through `tools oqlSearch --queries`, both with `--json --compact` | First result path/line, `proofGrade`, captures/ranges, diagnostics, pagination, and `next.*` continuations are semantically equivalent |
| OQL-22 | Direct diff | Can OQL compare one file across two refs without PR context? | `{"target":"diff","from":{"kind":"github","repo":"bgauryy/octocode-mcp"},"params":{"baseRef":"main","headRef":"main","path":"README.md"}}` | Returns a direct diff row with base/head/path and either a patch or an explicit identical-files diagnostic |

## Advanced Flow Rows

| ID | Flow | Steps | Measure |
|---|---|---|---|
| FLOW-01 | Search then exact fetch | Run OQL-03, then execute one returned `next.fetch` query | Fetched content is the exact source region around `_streamChatModelEvents`; repo/path/match anchor are preserved |
| FLOW-02 | Code search then PR archaeology | `search "streamEvents" LCJS --type ts` -> `history LCJS/<path>` -> `pr LCJS#<number> --deep` | The PR explains why the code changed; commit/PR/file paths line up |
| FLOW-03 | Package to source | `pkg zustand` -> repo URL -> OQL structure/content on `pmndrs/zustand` | npm and GitHub surfaces agree on source repo and package manifest |
| FLOW-04 | Cross-repo streaming comparison | Batch OQL code search for `streamEvents` in `LCJS` and `LGJS` | Results are separated by repo and preserve comparable paths/snippets |
| FLOW-05 | Python/TS graph comparison | Search `StateGraph` in `LGPY` and `LGJS`, then fetch one source file from each | Cross-language result rows do not collapse repo/language context |
| FLOW-06 | Zustand state API deep dive | OQL file search for `src/vanilla.ts` -> content symbols -> exact match fetch for `createStore` | Minified orienting step is cheaper than exact read; exact read still proves behavior |
| FLOW-07 | Hermes engine structure | Structure `HERMES_ENGINE` root and fetch `CMakeLists.txt` range | Non-TS repo browsing and file reads work; no TS-specific assumptions leak |
| FLOW-08 | Hermes agent structure | Structure `HERMES_AGENT`, then find package manifests and Python entrypoints | Mixed-language repo rows retain path and entry type |
| FLOW-09 | OpenClaw candidate handling | Repo search for `open claw` -> choose top relevant candidate -> structure root | If the selected candidate is unavailable, the response is scored on honest diagnostics and fallback to another candidate |
| FLOW-10 | PR comments pagination | PR detail with `itemsPerPage:5`, then `commentPage:2` when available | Comments/review comments page independently from body/patches |
| FLOW-11 | Large tree pagination | Structure `LCPY` or `LGPY` with `itemsPerPage:5`, follow `next.page` | Page 2 preserves repo/path/depth and returns a different window |
| FLOW-12 | Match pagination | Local/OQL code search for a common token with `maxMatchesPerFile:5`, then `matchPage:2` | Per-file match pages continue within the same file without losing result pages |
| FLOW-13 | Remote structural proof | OQL structural query on a bounded GitHub subtree with `materialize.mode:"required"` | Provider-only candidate output is not accepted as proof; materialized local AST proof is required |
| FLOW-14 | Binary strings pivot | `binary <native> --strings --min-length 12 --json` -> grep one returned string in the unpacked/source tree when applicable | Strings rows page by scan offset and can be used as research pivots |
| FLOW-15 | Archive after unzip | `unzip sample.tgz` -> `ls <localPath>` -> `find package.json <localPath>` -> `grep fixture <localPath>` -> `cat <localPath>/archive-src/package.json` | The unpack output becomes a normal local corpus for all local commands |

## Pagination Checks

| Domain | Required check |
|---|---|
| Result rows | `page`/`itemsPerPage` returns stable page windows and carries filters forward |
| GitHub provider | `currentPage`, `perPage`, `hasMore`, and next-page query are available when GitHub has more rows |
| PR detail | Body, patches, changed files, comments, reviews, and commits page independently where supported |
| Content | `charOffset`/`charLength` or line range is explicit; partial output advertises continuation |
| Search matches | `matchPage` and `maxMatchesPerFile` page within a noisy file |
| Archive entries | `entryPageNumber` and `entriesPerPage` page archive listings |
| Binary strings | `scanOffset`/`nextScanOffset` continues large binary scans |
| Research/graph | Packet pages keep the same corpus, facets, and intent |

## Token Efficiency Checks

| Scenario | Efficient path | Failing pattern |
|---|---|---|
| Unknown repo area | `ls`/structure depth 1-2 before `cat` | Full file reads before locating likely paths |
| Code search | `grep --concise` or OQL discovery before detailed snippets | Large snippets for broad terms |
| Exact proof | `cat --match-string` or OQL `fetch.content.match` | Full-content reads for one symbol |
| PR archaeology | PR list -> selected PR -> selected files/comments | `--deep` on broad PR lists |
| Minified/large files | `symbols` or `compact` first, exact range second | Exact full content without range or char window |
| Archives | `binary --list` before `--extract` or `unzip` | Guessing archive entries |
| Binary strings | Raise `--min-length`, use offsets, then pivot | Dumping all strings without pagination |
| OQL cross-repo | Batch up to 5 independent scoped queries | One huge unscoped query with merged ambiguous rows |

## Raw Tool To OQL Parity

Use this table to compare raw tool output against OQL output. OQL does not need
to mirror every envelope byte-for-byte; it must preserve the information needed
to continue the same research.

| Raw capability | OQL target | Required parity |
|---|---|---|
| `ghSearchCode` content | `code` | Repo/path/snippet/match offsets/query context and next fetch |
| `ghSearchCode` path | `files` | Repo/path/file identity and page continuation |
| `ghGetFileContent` | `content` | Exact/compact/symbol views, line/match/char pagination |
| `ghViewRepoStructure` | `structure` | Directory entries, depth/scope, page continuation |
| `ghSearchRepos` | `repositories` | Candidate identity, filters, sort/page context |
| `ghHistoryResearch` PRs | `pullRequests`/`diff` | PR list/detail, review/comment/commit/file pagination, selected patch reads |
| `ghHistoryResearch` commits | `commits` | Commit list, path/branch/page context |
| `ghCloneRepo` | `materialize` | Local checkpoint, source repo/ref, follow-up local queries |
| `npmSearch` | `packages` | Package identity, version, repository handoff |
| `localSearchCode` | `code` | Text/regex/structural semantics, context, match pages |
| `localFindFiles` | `files` | Name/path/attribute predicates and page continuation |
| `localGetFileContent` | `content` | Range/match/char reads and minification |
| `localViewStructure` | `structure` | Local tree rows and details |
| `localBinaryInspect` | `artifacts` | Inspect/list/extract/decompress/strings/unpack modes and continuations |
| `lspGetSemantics` | `semantics` | All semantic query types with line anchors and capability diagnostics |
| `oqlSearch` | all active OQL targets | Same OQL result semantics as `octocode search --query`; transport wrapper differences are allowed, but row data, proof grade, diagnostics, pagination, and continuations must match |

### Gold-Trace A/B Tasks

Run each task twice with the same model, same prompt, same budget, and same
corpus. Arm A may use the raw CLI command row or raw MCP/tool row listed below.
Arm B must use the OQL row. Score the final answer and trace, not just the
first response. OQL loses the row if any transformer mapping silently drops a
predicate, scope, paging field, match anchor, or proof limitation.

| ID | Source rows | Same prompt | Arm A: raw CLI/MCP | Arm B: OQL | Gold trace |
|---|---|---|---|---|---|
| AB-01 | CLI-08, TOOL-02, OQL-03/OQL-04 | Find where LangChain.js handles streaming events and show one follow-up read. | `grep` or `ghSearchCode` | `target:"code"` | Repo `langchain-ai/langchainjs`, TypeScript scope, code rows with path/snippet/match context, executable fetch continuation |
| AB-02 | CLI-09, TOOL-03, OQL-05 | Find Zustand package manifests without reading file bodies. | `find` or `ghSearchCode` path mode | `target:"files"` | Path-level rows for `package.json`, no snippet-only dependency, page context preserved |
| AB-03 | CLI-07, TOOL-04/TOOL-05, OQL-07/OQL-08 | Read Zustand `createStore` with exact proof after a cheap orienting view. | `cat` / `ghGetFileContent` | `target:"content"` | Symbols/compact view is cheaper; exact match read preserves line or range anchors |
| AB-04 | CLI-06, TOOL-06, OQL-06 | Browse LangGraph.js `libs` cheaply and continue to page 2 if needed. | `ls` or `ghViewRepoStructure` | `target:"structure"` | Depth/path filters and `itemsPerPage` survive into `next.page` |
| AB-05 | CLI-05, TOOL-01, OQL-09 | Discover benchmark repo candidates across LangChain, LangGraph, Zustand, OpenClaw, and Hermes. | `repo` or `ghSearchRepos` batch | `target:"repositories"` batch | Five independent result groups, candidate identity, filters, no merged ambiguity |
| AB-06 | CLI-13, TOOL-11, OQL-10 | Resolve npm `zustand` to source repo and package metadata. | `pkg` or `npmSearch` | `target:"packages"` | Package version plus repository/source handoff fields |
| AB-07 | CLI-10, TOOL-07, OQL-11 | Find merged LangChain.js PR candidates for streaming events. | `pr` list or `ghHistoryResearch` PR mode | `target:"pullRequests"` | PR number/title/state/date, query context, page continuation |
| AB-08 | CLI-11, TOOL-08, OQL-12/OQL-14 | Deep-read LangChain.js PR `10924` around `_streamChatModelEvents`. | `pr --deep` or `ghHistoryResearch` detail | `pullRequests` plus `diff` | Body, comments, reviews, commits, changed files, scoped diff, honest independent pagination |
| AB-09 | CLI-12, TOOL-09, OQL-13 | Read commit history for `libs/langchain-core`. | `history` or `ghHistoryResearch` commits | `target:"commits"` | Commit sha/date/author/message, path scope, next page when present |
| AB-10 | TOOL-15, OQL structural code path | Prove local structural matches for `diagnostic($$$ARGS)` in OQL source. | `localSearchCode mode:"structural"` | `target:"code"` structural predicate | AST-backed rows, captures/ranges when available, `proofGrade:"structural"`, no string/comment false positives |
| AB-11 | TOOL-18, OQL-16 | Get document symbols for the OQL runner. | `lspGetSemantics` | `target:"semantics"` | Symbol names/kinds/ranges, `proofGrade:"semantic"`, capability diagnostics if LSP is unavailable |
| AB-12 | CLI-14/CLI-15, TOOL-17, OQL-19 | List archive entries, then pivot to local research after extraction. | `binary`/`unzip` or `localBinaryInspect` | `target:"artifacts"` | Archive entry rows, pagination/scan continuation, extracted local path usable by local follow-up |
| AB-13 | TOOL-19, OQL-21 | Compare the raw OQL tool with the local CLI `search` command for the same query. | `tools oqlSearch --queries` | `search --query` | Same first structural row, `metavars`, `proofGrade`, diagnostics, pagination, and executable continuations; no CLI-only transformation changes full JSON OQL semantics |

## Result Sheet Columns

Record run results outside this doc with these columns:

| Column | Meaning |
|---|---|
| `id` | Row ID from this eval |
| `surface` | CLI, tool, OQL, or flow |
| `score` | `2`, `1`, `0`, or `N/A` |
| `data_present` | Did the command return meaningful rows or a valid explicit refusal? |
| `followup_ok` | Could the next query be executed without guessing? |
| `pagination_ok` | Did pagination continue correctly where applicable? |
| `token_ok` | Was the cheapest adequate view used first? |
| `diagnostics_ok` | Were empty, partial, unsupported, or provider-limited cases honest? |
| `proofGrade_ok` | Did every OQL row carry a valid `proofGrade`, and did it match the evidence used? |
| `lossy_transform_ok` | Did every lossy/unsupported transformer mapping emit a blocking diagnostic instead of silently narrowing? |
| `notes` | Short actionable note only |
