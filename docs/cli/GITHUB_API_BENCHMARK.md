# GitHub API CLI Benchmark

Run date: 2026-06-22

This benchmark checks the GitHub-facing Octocode CLI paths against the raw
GitHub tools exposed through `octocode tools <name> --queries`. It is meant to
catch routing gaps between the friendly commands, OQL `search`, and the direct
tool APIs.

## Scope

Target repositories:

| Project | Repository | Code probe | Structure path | Content probe |
|---|---|---|---|---|
| LangChain | `langchain-ai/langchainjs` | `BaseChatModel` | `libs/langchain-core/src/language_models` | `libs/langchain-core/src/language_models/chat_models.ts` |
| Zustand | `pmndrs/zustand` | `createStore` | `src` | `src/vanilla.ts` |
| Vue | `vuejs/core` | `createApp` | `packages/runtime-core/src` | `packages/runtime-core/src/apiCreateApp.ts` |
| Next.js | `vercel/next.js` | `NextRequest` | `packages/next/src/server/web/spec-extension` | `packages/next/src/server/web/spec-extension/request.ts` |

APIs and CLI paths covered:

| Area | Friendly command | OQL `search` target | Raw tool |
|---|---|---|---|
| Repository discovery | `search --target repositories` | `repositories` | `ghSearchRepos` |
| Code search | `search` | `code` | `ghSearchCode` |
| Repository structure | `search --tree` | `structure` | `ghViewRepoStructure` |
| Content fetch | `search <file>` | `content` | `ghGetFileContent` |
| PR search | `search --target pullRequests` | `pullRequests` | `ghHistoryResearch` |
| PR deep dive | `search <repo>#<n> --target pullRequests` | `pullRequests` with `prNumber` | `ghHistoryResearch` with `prNumber` |

## Environment

The repo was built before the run:

```bash
PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH" yarn build
```

The Codex app-bundled Node rejected the native engine addon on macOS, so the
benchmark used Homebrew Node:

```bash
PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH" node packages/octocode/out/octocode.js status --json
```

GitHub auth was available through the `gh-cli` token for user `bgauryy`.

## Command Patterns

Repository discovery:

```bash
node packages/octocode/out/octocode.js search <term> --target repositories --limit 3 --json
node packages/octocode/out/octocode.js search --query '{"target":"repositories","params":{"keywords":["<term>"],"limit":3},"view":"discovery"}' --json --compact
node packages/octocode/out/octocode.js tools ghSearchRepos --queries '{"keywords":["<term>"],"limit":3,"concise":true}' --json --compact
```

Code search:

```bash
node packages/octocode/out/octocode.js search "<symbol>" <owner/repo> --lang ts --json --compact
node packages/octocode/out/octocode.js tools ghSearchCode --queries '{"owner":"<owner>","repo":"<repo>","keywords":["<symbol>"],"extension":"ts","limit":5,"concise":true}' --json --compact
node packages/octocode/out/octocode.js search <symbol> <path> --repo <owner/repo> --lang ts --view discovery --limit 5 --json
```

Structure and content:

```bash
node packages/octocode/out/octocode.js search --query '{"target":"structure","from":{"kind":"github","repo":"<owner/repo>"},"scope":{"path":"<path>"},"limit":10}' --json --compact
node packages/octocode/out/octocode.js tools ghViewRepoStructure --queries '{"owner":"<owner>","repo":"<repo>","path":"<path>","maxDepth":2,"itemsPerPage":10}' --json --compact
node packages/octocode/out/octocode.js search --query '{"target":"content","from":{"kind":"github","repo":"<owner/repo>"},"scope":{"path":"<file>"},"fetch":{"content":{"contentView":"symbols","charLength":5000}}}' --json --compact
node packages/octocode/out/octocode.js tools ghGetFileContent --queries '{"owner":"<owner>","repo":"<repo>","path":"<file>","minify":"symbols","charLength":5000}' --json --compact
```

Pull requests:

```bash
node packages/octocode/out/octocode.js search <owner/repo> --target pullRequests --query docs --state merged --limit 1 --json
node packages/octocode/out/octocode.js search --query '{"target":"pullRequests","from":{"kind":"github","repo":"<owner/repo>"},"params":{"state":"merged","keywordsToSearch":["docs"],"limit":1},"view":"discovery"}' --json --compact
node packages/octocode/out/octocode.js tools ghHistoryResearch --queries '{"type":"prs","owner":"<owner>","repo":"<repo>","state":"merged","keywordsToSearch":["docs"],"limit":1,"concise":false}' --json --compact
node packages/octocode/out/octocode.js search <owner/repo>#<number> --target pullRequests --patches --comments --commits --char-length 3000 --items-per-page 5 --json
node packages/octocode/out/octocode.js tools ghHistoryResearch --queries '{"type":"prs","owner":"<owner>","repo":"<repo>","prNumber":<number>,"content":{"metadata":true,"changedFiles":true,"patches":{"mode":"all"},"comments":{"discussion":true,"reviewInline":true,"includeBots":false},"reviews":true,"commits":{"list":true}},"charLength":3000,"itemsPerPage":5,"minify":"standard"}' --json --compact
```

## Results

Summary:

| Metric | Result |
|---|---|
| Commands run | 68 |
| Target repos | 4 |
| Hard failures | 0 reproducible |
| Transient harness timeout | 1, `Zustand OQL PR list`; direct rerun succeeded in 1.826s |
| Consistent diagnostics | 4, all `search` target `code` returning `providerUnindexed` |

Per-repo results:

| Project | Repo discovery | OQL code search | Raw code search | Materialized search | Structure | Content | PR list and deep read |
|---|---|---|---|---|---|---|---|
| LangChain | OK, top hit `langchain-ai/langchainjs` | Empty, `providerUnindexed` | OK, 67 reported matches; first file `libs/langchain-core/src/language_models/chat_models.ts` | OK, found `chat_models.ts` | OK, paginated | OK, symbols fetched for `chat_models.ts` | OK, PR `#11067` deep-read |
| Zustand | OK, top hit `pmndrs/zustand` | Empty, `providerUnindexed` | OK, 4 files; first file `src/vanilla.ts` | OK, 3 files | OK | OK, symbols fetched for `src/vanilla.ts` | OK, PR `#3527` deep-read |
| Vue | OK, search term returned `vuejs/vue` first; benchmark target was explicit `vuejs/core` | Empty, `providerUnindexed` | OK, 74 reported matches; first file `packages/runtime-core/src/apiCreateApp.ts` | OK, 5 files | OK, paginated | OK, symbols fetched for `apiCreateApp.ts` | OK, PR `#14918` deep-read |
| Next.js | OK API call, but `nextjs` did not return `vercel/next.js` in the top 3 | Empty, `providerUnindexed` | OK, 208 reported matches; first file `packages/next/src/server/web/spec-extension/request.ts` | OK, 2 files | OK, paginated | OK, symbols fetched for `request.ts` | OK, PR `#94719` deep-read |

Selected PRs:

| Project | PR | Title |
|---|---:|---|
| LangChain | `#11067` | `docs(tavily): fix "refering" typo in tool description` |
| Zustand | `#3527` | `docs: fix missing code highlights in tic-tac-toe tutorial` |
| Vue | `#14918` | `docs: update Vite documentation links` |
| Next.js | `#94719` | `[agents-md] Index bundled docs instead of downloading into .next-docs` |

## Findings

1. The 2026-06-22 run exposed an OQL GitHub code-lowering gap: friendly `search "<symbol>" <owner/repo> --lang ts` returned `providerUnindexed` while raw `ghSearchCode` found matching files immediately and materialized local search proved the same matches. Current 2026-06-24 CLI checks narrow that gap: friendly `--lang ts`, raw `ghSearchCode`, and canonical JSON with `scope.language:"ts"` return the expected hits; the remaining bug was canonical JSON with general names such as `scope.language:"TypeScript"` blocking as `lossyTransform` instead of passing the GitHub `language` qualifier through to `ghSearchCode`.

2. Raw GitHub APIs are healthy through the CLI tool runner. `ghSearchRepos`, `ghSearchCode`, `ghViewRepoStructure`, `ghGetFileContent`, and `ghHistoryResearch` all returned expected results through `tools <name> --queries`.

3. OQL non-code GitHub targets are healthy in this matrix. `repositories`, `structure`, `content`, and `pullRequests` matched the raw tools closely. Paginated discovery targets correctly reported partial evidence, while content fetches reported proof-grade evidence.

4. Friendly GitHub flows line up with raw tools for structure, content, and PRs. `search --tree`, `search <file>`, and `search --target pullRequests` produced the same practical answers as their raw tool equivalents.

5. Repository discovery depends strongly on query wording. `search vue --target repositories --lang TypeScript` and `search nextjs --target repositories --lang TypeScript` exercised the API successfully but did not surface the benchmark target repos in the top three. The follow-up checks used explicit owner/repo targets.

6. The only timed issue was not reproducible. The harness timed out once on `Zustand OQL PR list` after 60 seconds with no output; the same command run directly returned PR `#3527` in 1.826 seconds.

Current fix target: keep OQL GitHub code adapter parity with raw `ghSearchCode` for provider-supported text queries, including general language names (`TypeScript`) and extension selectors (`ts`). Results from GitHub code search should remain candidate-grade provider evidence until exact content or materialized local proof is followed.
