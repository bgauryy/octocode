# Octocode flow benchmark questions

Flow rows score whether an agent can move through `orient -> search -> read -> prove` without guessing. Each flow should record command timings, output tokens, cache mode, data completeness, pagination, continuation quality, and reflection.

Candidate/provider rows are allowed early in a flow, but the final answer must not call them proof unless the flow follows an exact fetch, materialized local proof, LSP result, graph proof, selected diff, or PR detail row.

## Local Workflows

| ID | Flow | Question | Steps | Pass criteria |
|---|---|---|---|---|
| FLOW-LCL-01 | Search to exact content | Can a local text search produce a match anchor that fetches the exact region? | `search "runOqlSearch" packages/octocode/src/cli/commands/search.ts --json --compact` -> follow `next.fetch` or run `search packages/octocode/src/cli/commands/search.ts --match-string runOqlSearch --content-view exact`. | Final content includes exact source, line anchors, and no hidden truncation. |
| FLOW-LCL-02 | Search to LSP | Can local search feed semantic navigation without guessed lines? | `search <file> --op documentSymbols` -> select `searchCommand` line -> `search <file> --op references --symbol searchCommand --line <line>`. | `line` comes from symbols; references are real or capability diagnostic is honest. |
| FLOW-LCL-03 | Structural to proof | Can AST search find a code shape and then fetch exact context? | `search --pattern 'getString($$$ARGS)' packages/octocode/src/cli/commands/search.ts --lang ts --json --compact` -> content fetch around one returned line. | AST result is structural/candidate as appropriate; final content proves the actual call. |
| FLOW-LCL-04 | Match pagination | Can a noisy local file page through matches without losing context? | `tools localSearchCode` on `search.ts` with `maxMatchesPerFile:5, matchPage:1` -> follow `matchPage:2`. | Page 2 keeps file/filter context and returns a different match window. |
| FLOW-LCL-05 | Content pagination | Can large local content page by char window? | `localGetFileContent` with `charLength:4000` -> follow `pagination.nextCharOffset`. | Page 2 is lossless and explicit; no unmarked truncation. |
| FLOW-LCL-06 | Structure to targeted file | Can tree browsing narrow before reading? | `search packages/octocode-tools-core/src/oql --tree --depth 2 --json --compact` -> selected `search <path> --content-view exact`. | First step is cheaper than full read; final evidence has exact path/lines. |
| FLOW-LCL-07 | Files to content | Can file discovery find manifests without content, then read one exact file? | `search package.json . --target files --name package.json --json --compact` -> `search <returned path> --content-view exact --json --compact`. | File identity survives the handoff; no content read during discovery. |
| FLOW-LCL-08 | Archive to local corpus | Can archive inspection turn into normal local research? | `search sample.tgz --target artifacts --list` -> `search sample.tgz --target artifacts --extract archive-src/package.json` or `--artifact-mode unpack` -> `search` on returned `localPath`. | Entry names are not guessed; unpacked output is usable by search/OQL local targets. |
| FLOW-LCL-09 | Binary strings pivot | Can strings mode provide bounded pivots? | `search <native.node> --target artifacts --strings --min-length 12 --json` -> page with `scanOffset` or search a returned string in source when applicable. | Output is paginated; strings are usable as pivots without dumping everything. |
| FLOW-LCL-10 | Research to graph proof | Can OQL research summarize first and upgrade later? | `target:"research", page:1, itemsPerPage:1` -> page packets -> run row `next.graph` with `proof:"lsp"`. | Summary appears before bulk data; graph proofStatus is explicit; `answerReady:false` is understood. |
| FLOW-LCL-11 | Search CLI parity | Do quick CLI, raw local tool, and OQL agree for one local question? | Compare `search "runOqlSearch" ...`, `tools localSearchCode ...`, and `tools oqlSearch ...`. | Envelopes may differ, but path/line/snippet/evidence/continuation agree. |

## External Workflows

| ID | Flow | Question | Steps | Pass criteria |
|---|---|---|---|---|
| FLOW-EXT-01 | GitHub search to exact fetch | Can provider code search produce a runnable content fetch? | `search "_streamChatModelEvents" langchain-ai/langchainjs --lang ts --json --compact` -> follow `next.fetch` or `ghGetFileContent matchString`. | Final content is exact proof around the match; repo/path/ref/match anchor are preserved. |
| FLOW-EXT-02 | Path search to content | Can path-level discovery avoid snippets before reading? | `tools ghSearchCode --queries '{"keywords":["package.json"],"owner":"pmndrs","repo":"zustand","match":"path","limit":10}' --json --compact` or `search "package.json" pmndrs/zustand --search path --materialize auto --json --compact` -> fetch one returned manifest. | Discovery rows are path-only; final read proves selected file. |
| FLOW-EXT-03 | Package to source | Can npm package metadata hand off to GitHub source proof? | `search zustand --target packages --json` -> repo URL -> `search --target structure/content` on `pmndrs/zustand`. | npm and GitHub source identity agree; package version and repo path are recorded. |
| FLOW-EXT-04 | Repo discovery to browse | Can repository search select a candidate and browse it cheaply? | `search "open claw" --target repositories --json --compact` -> choose candidate -> `search owner/repo --tree --depth 1 --json --compact`. | Candidate ambiguity is explicit; selected repo is browseable or failure is honest. |
| FLOW-EXT-05 | PR archaeology | Can code search lead to commit history and PR context? | GitHub code search for `streamEvents` -> `search owner/repo --target commits --path <path>` -> selected `search owner/repo#<number> --target pullRequests --deep`. | PR/commit/file paths line up; final PR explains why behavior changed. |
| FLOW-EXT-06 | PR detail pagination | Can large PR sections page independently? | `search langchain-ai/langchainjs#10924 --target pullRequests --deep --match-string "_streamChatModelEvents" --json --compact` with small `--items-per-page`, then page comments/files/commits/body. | Each continuation keeps PR number, match filter, and content selector. |
| FLOW-EXT-07 | Commit history to PR | Can commit headlines feed PR detail? | `search langchain-ai/langchainjs --target commits --path libs/langchain-core --limit 5 --json` -> extract `(#NNN)` when present -> `search langchain-ai/langchainjs#NNN --target pullRequests --deep --json --compact`. | Commit SHA/date/path and PR number are preserved; absence of PR number is not guessed. |
| FLOW-EXT-08 | Clone/materialize to local proof | Can a bounded remote subtree become a local corpus for AST/LSP? | `search --query '{"target":"materialize",...}'` or `clone pmndrs/zustand/src` -> local `search --pattern`, `search <file> --content-view exact`, and `search <file> --op ...` on returned path. | Materialization returns localPath/repoRoot/ref; local proof uses the returned path. |
| FLOW-EXT-09 | Remote-as-local quick commands | Do `--repo` commands reuse a remote local cache consistently? | `search "createStore" src --repo pmndrs/zustand --lang ts` -> `search src/vanilla.ts --repo pmndrs/zustand --match-string createStore --content-view exact`. | `location.*` reports cached/complete/verified state; repo-relative path survives. |
| FLOW-EXT-10 | Cross-repo comparison | Can batched OQL compare two repos without merging identities? | Batch `target:"code"` for `streamEvents` in `LCJS` and `LGJS`, or `StateGraph` in `LGPY` and `LGJS`. | Rows remain separated by repo/language/path; comparable fields are present. |
| FLOW-EXT-11 | Non-TypeScript repo browse | Can remote structure/content avoid TypeScript assumptions? | `search facebook/hermes --tree --depth 1 --json --compact` -> fetch `CMakeLists.txt` range or search native path. | Non-TS files are readable; language-specific hints do not mislead. |
| FLOW-EXT-12 | Direct diff | Can OQL compare a single file across two refs without PR context? | `search --query '{"target":"diff","from":{"kind":"github","repo":"bgauryy/octocode-mcp"},"params":{"baseRef":"main","headRef":"main","path":"README.md"}}' --json --compact`. | Diff row carries base/head/path and patch or identical-files diagnostic. |
| FLOW-EXT-13 | OQL parity | Does raw `oqlSearch` match search CLI for the same full query? | Run the same OQL object through `octocode search --query` and `tools oqlSearch --queries`. | First rows, evidence, diagnostics, pagination, and continuations are semantically equivalent. |
| FLOW-EXT-14 | Search route honesty | Does dry-run detect lossy/unsupported mappings before execution? | `search --explain --dry-run --query <GitHub regex/structural/materialize query> --json`. | Plan emits PUSHDOWN/RESIDUAL/ROUTE/UNSUPPORTED and blocking diagnostics for lossy transforms. |

## Flow Rating Rubric

| Rating dimension | 10/10 behavior |
|---|---|
| Time | Local flows report median after warmup; provider flows record live latency and cache/auth state. |
| Token use | Cheap orientation (`--compact`, discovery, path-only, symbols) happens before detailed content. |
| Data quality | Every final claim has exact file/line/repo/package/PR/commit/artifact anchors. |
| Pagination | Every partial page has a runnable continuation and page 2 preserves scope. |
| Match handoff | Search rows feed fetch/LSP/PR/diff rows through explicit anchors, not human reconstruction. |
| Schema quality | Fields used in the flow are documented by help/scheme and match the actual accepted shape. |
| Error honesty | Empty, unsupported, approximate, auth-gated, or rate-limited states include repair guidance. |
| Reflection quality | Issues, improvements, good flows, missing instructions, and next fix are concrete. |

## Reflection Prompts

Use these prompts after every flow run:

| Prompt | Answer with |
|---|---|
| Best flow | Which chain gave the strongest proof for the fewest output tokens and why? |
| Issues | Which command failed, paginated badly, hid data, or required guessing? |
| Possible improvements | What product change would reduce time, tokens, or ambiguity? Include reason. |
| Good flows | Which sequence should be copied into docs or examples? |
| Missing instructions | What schema/help/agent instruction was missing, contradictory, stale, or not working? |
| Next fix | One concrete code, docs, or benchmark automation task. |
