# Questions

17 GitHub research questions. Each question is assigned a **tool category** that names the capability being evaluated. Answer each one, in order, using only the tool you were assigned.

There is intentionally no answer key file. The judge independently validates each submitted answer against the live GitHub repositories and PRs.

Questions tagged `[drift]` are time-sensitive (counts, recent activity). The judge scores them loosely and reports them separately.

---

## Tool Categories And Capability Dimensions

| Category | Tag | Octocode tool | gh comparison surface | Questions |
|---|---|---|---|---|
| Code search completeness | `SEARCH` | `githubSearchCode` | Result limits and multi-query workflows | Q1–Q4 |
| File content completeness | `CONTENT` | `githubGetFileContent` | Large-file retrieval and targeted reads | Q5–Q8 |
| Repo tree navigation | `STRUCTURE` | `githubViewRepoStructure` | Tree shape, filtering, and metadata extraction | Q9–Q11 |
| PR intelligence | `PR` | `githubSearchPullRequests` | PR comments, reviews, commits, and changed files | Q12–Q15 |
| Repository search | `REPOS` | `githubSearchRepositories` | Search filters, counts, and pagination metadata | Q16–Q17 |

---

### Q1 — Exhaustive code search: result-limit coverage `[SEARCH]`

In `vercel/next.js`, find every file that imports from the string `'react'` (exact import).
1. How many unique files contain this import?
2. List the first 10 file paths.

> *Evaluates exhaustive code-search retrieval and pagination when the true match count may exceed a single response.*

---

### Q2 — Multi-repo bulk search efficiency `[SEARCH]`

Find where the core state primitive is **defined** (not just used) in each of these repos:
- `facebook/react` — find where `useState` hook is implemented
- `vuejs/core` — find where `ref()` is implemented
- `solidjs/solid` — find where `createSignal` is implemented

For each: state the file path and the line of the function definition.

> *Tests bulk query efficiency. Octocode accepts multiple queries in a single call. `gh search code` issues one query per invocation. The judge counts total calls and total chars for each agent separately.*

---

### Q3 — Search with textMatch context `[SEARCH]`

In `honojs/hono`, find every call to `compose()` inside `src/`.
For each match: state the exact file path, line number, and the exact line of source code where `compose` is called.

> *Tests the quality of match context returned alongside search results. The judge verifies whether line numbers are present and whether the returned code line is correct.*

---

### Q4 — Multi-keyword narrowing search `[SEARCH]`

In `vercel/next.js`, find files that contain **both** `ppr` and `Postpone` in the same file.
1. How many files match?
2. List all file paths.

> *Tests AND-intersection query semantics. `gh search code` keyword matching is OR-union by default. Tests whether each agent can express a file-level AND constraint.*

---

### Q5 — Large file: targeted section read `[CONTENT]`

Read `packages/react-reconciler/src/ReactFiberWorkLoop.js` in `facebook/react`.
1. What are the names of the top-level exported functions?
2. What is the purpose of the `performConcurrentWorkOnRoot` function according to its signature and immediate context?

> *Tests targeted windowed reads. The file is several thousand lines. `gh api` fetches the entire base64 blob to answer a question about two specific symbols. Octocode can use a `matchString` anchor to retrieve only the relevant sections.*

---

### Q6 — Large file: extract from the END `[CONTENT]`

Read `packages/vite/CHANGELOG.md` in `vitejs/vite` completely.
1. What was the **first** `4.x` release version and what did it change?
2. What is the latest release version listed?

> *Tests large-file tail reads. The CHANGELOG is ~231 KB and the oldest entries are at the bottom of the file. Octocode supports char-offset windows to seek directly to the tail. Also tests path discovery in a monorepo structure.*

---

### Q7 — Over-size-limit file `[CONTENT]`

Read `src/compiler/checker.ts` in `microsoft/TypeScript`.
1. What does the first exported function (`createTypeChecker` or equivalent) do?
2. What does the last function in the file do?

> *Evaluates handling of files that exceed the GitHub `/contents/` inline size limit. Large files may require blob retrieval or char-offset pagination.*

---

### Q8 — Directory listing via content API `[CONTENT]`

List all files directly inside `packages/react/src/` in `facebook/react`.
1. How many total files are in this directory (not recursive)?
2. Which files have a `.ts` extension vs `.js`?

> *Tests directory listing efficiency. `gh api` on a directory path returns a flat JSON array of entry objects (no `content` field, but no type filtering or metadata grouping either). Octocode returns a structured listing with extension and type metadata.*

---

### Q9 — Subtree file count `[STRUCTURE]`

In `vuejs/core`, how many `.ts` source files exist anywhere under `packages/reactivity/src/`?
List all of them by file name.

> *Evaluates filtered subtree navigation and how much parsing/filtering each toolset asks the agent to perform.*

---

### Q10 — Two-repo structure comparison `[STRUCTURE]`

Compare the top-level source directory structure of `honojs/hono` (`src/`) vs `expressjs/express` (`lib/`).
1. How many subdirectories does each have at the top level of their source root?
2. What does the naming and organization difference reveal about each project's architectural approach?

> *Tests bulk structure queries. Octocode can retrieve both trees in a single call. `gh` requires two separate API requests. Also tests navigation to different source root conventions (`src/` vs `lib/`).*

---

### Q11 — Entry-point discovery from structure `[STRUCTURE]`

In `vitejs/vite`, trace the dev server entry point starting from `packages/vite/src/`.
1. Which file is the main entry point for the dev server?
2. What is the first function it calls on startup?

> *Evaluates a two-step workflow: tree navigation followed by file content read. Both agents first discover the right file via structure, then read it. The judge evaluates completeness and accuracy of both steps.*

---

### Q12 — PR labels in search results `[PR]`

In `vercel/next.js`, find merged PRs carrying the label `Pages Router` that were merged since January 2025. `[drift]`
1. How many such PRs exist?
2. List the 5 most recent: PR number, title, and merged date.

> *Tests whether labels are accessible in PR search results. `gh search prs` requires explicit `--json labels` to include label data and returns a limited default result count. Octocode includes labels in structured results by default.*

---

### Q13 — Inline review thread comments `[PR]`

For PR #27733 in `facebook/react`:
1. How many **inline review comments** (code-level thread comments, not PR-level review summaries) are there?
2. Quote the single most substantive objection raised by a reviewer in those inline comments.
3. Which file did the most review comments target?

> *Tests access to inline review thread comments. `gh pr view --json reviews` returns PR-level review summaries only. Inline thread comments are a separate API resource. Octocode's `withComments: true` retrieves both in one call.*

---

### Q14 — PR commits: full commit list `[PR]`

In `honojs/hono`, find the merged PR that introduced the `hono/jsx` package or JSX runtime support.
1. How many commits does the PR contain?
2. List every commit SHA and its message.
3. Who authored the commits?

> *Tests complete commit history retrieval. GitHub paginates commits beyond a threshold and `gh pr view --json commits` does not auto-paginate. Octocode's `withCommits: true` fetches the full list.*

---

### Q15 — PR archaeology: find the introducing PR `[PR]`

In `honojs/hono`, find the first merged PR that introduced JSX / JSX renderer support.
1. What is the PR number and title?
2. What was the stated motivation in the PR body?
3. Which files were added or changed to implement it?

> *Tests PR search combined with body and diff access. Retrieving PR body and changed-file list via `gh` requires explicit field selection across multiple flags. Octocode uses pay-per-field parameters (`withDiff: true`) in a single call.*

---

### Q16 — Multi-filter repository search `[REPOS]`

Find TypeScript repositories with the topic `mcp` that have at least 500 stars and were updated since 2025. `[drift]`
1. How many match?
2. List the top 5 by star count: name, star count, description.

> *Tests structured multi-filter search with pagination. Both tools support the individual filters. The judge evaluates whether result counts are complete and whether pagination metadata is exposed.*

---

### Q17 — Enumerate all repos in an organization `[REPOS]` `[drift]`

List every public repository in the `vercel` GitHub organization.
1. How many total public repos does the org have?
2. Which 5 have the most stars? State name and star count.
3. How many repos have over 1,000 stars?

> *Evaluates exhaustive org-level enumeration, result limits, and pagination. The judge independently verifies the total via the GitHub API.*
