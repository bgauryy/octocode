# Checks

How to check Octocode: its core tools first, then security, skills and the areas other packages own.
Direct calls, judged by reading the output, compared with what `gh`, `rg`, `ast-grep` or a registry would
return. This file is the whole method for checks; [METHOD.md](METHOD.md) owns benchmark rounds and
[COMPARISONS.md](COMPARISONS.md) the expert commands. Read the owner's output rule first: a default answer
carries the answer, identity for the next call, one indication, and the way to get more; detail is one call
away and nothing is lost.

Set up the checking agent and its environment as [TESTING_ENV.md](TESTING_ENV.md) says, then check each tool with its
[questions](questions/tools/README.md): real research queries, the workflow into the tools it relates to, and one bad input.
The agent reads the description and schema as an agent would, makes the calls, follows the follow-ups the response offers,
and judges every answer by reading it.

For a rating that does not depend on one author's questions, run [independent raters](briefs/independent-raters.md): one fresh agent per tool group, fresh questions, every claim checked against ground truth, each tool scored on the [12-dimension rubric](RATINGS.md#agent-experience-rubric-12-dimensions).

A round fills in one [scorecard](briefs/SCORECARD.md), using the [1–10 rating rubric](RATINGS.md),
so what is ready is visible at a glance. Every tool has [specific questions](questions/tools/README.md).
Ready briefs
for a checking agent: [local vs smart rg and ast-grep](briefs/check-local.md), [GitHub and packages vs gh
lean](briefs/check-github.md), and the [local flows benchmark](local/FLOWS.md).

## Areas

| Area | Checked by | Owner of the checks |
|---|---|---|
| **Core tools** (first) | the 8-dimension review, end to end, per tool, workflows | this file: [Core tools](#core-tools) |
| **Security** | behavior under secrets, credentials, paths, providers and hostile text | this file: [Security](#security) |
| **Skills** | activation, steps, tool use, output, links, improvement loop | [SKILLS.md](SKILLS.md); `dev skills:check` is the mechanical gate |
| **Release gates** | lint, typecheck, tests, docs, skills, packaging | [AGENTS.md](../../AGENTS.md) |
| **Benchmark** | version against version and against stored baselines | [METHOD.md](METHOD.md), [local flows](local/FLOWS.md) |
| Acceptance criteria per tool | what "passes" means for schema, output, pagination, efficiency | [tool quality](../../skills-dev/octocode-dev/docs/TOOL_QUALITY.md) |
| Credentials, packaged | auth flows on the packaged binary | [packaged auth checks](../octocode-native/docs/GITHUB_AUTH_TESTS.md#packaged-checks) |
| Browser | the Chrome DevTools package | [browser checks](../octocode-chrome-devtools/README.md#browser-checks) |
| Graph languages | the grammar battery over the pinned clones | [graph battery](../../skills-dev/octocode-graph-dev/references/battery.md) |
| Agents communication | concurrent native workers | [worker checks](../octocode-agents-communication/docs/OPERATIONS.md#verify-concurrent-native-workers) |

The areas owned elsewhere use the same method (direct calls judged by reading) and report into the same
scorecard; their owners keep the details.

## Rules

- **Scripts own exact, repeated work; you own the judgment.** Pins, page walks, transcript tallies and the graph
  battery run as checked-in scripts (`dev pin`, `dev walk`, `harness-tally.mjs`, `battery.mjs`). Call the tool,
  read the answer, decide; do not write throwaway drivers that hard-code inputs.
- **Judge the output, not an exit code.** For each call ask: is the answer right, is it on page 1, can the
  next call be made from what is shown, is anything there that the question did not need?
- **Compare against the lean alternative an expert writes** (`gh … --json/--jq`, `rg -o`, `sed -n`,
  `curl | jq`), not the plainest command. Record characters (≈ 4 per token) and time; Octocode first call and
  repeat call separately.
- **Live GitHub targets:** `vitejs/vite` and other public repositories; not `facebook/react`. Use a second
  repository before calling a GitHub result a defect.
- **Paid and private:** clasify bills per request; keep a count. Never send anything secret-shaped to a real
  host; use a loopback server for anything near credentials.
- **Pinned binary:** check one binary, the one the round pinned, and name its hash in the report.
- **One home:** questions, common checks, campaign fixtures/receipts and ratings belong under this
  package. Put raw evidence in `campaigns/<round>/` and publish its result in `results/rounds/<round>/`.

How to call (CLI text is what agents read; `--json` shows structure; `schema <tool>` prints the schema,
`schema <tool> --select operation=X` one operation):

```bash
printf '%s' '{"queries":[{ … }]}' | NODE_ENV=development OCTOCODE_NATIVE_BIN=<abs path> \
  OCTOCODE_ENABLE_LOCAL=true OCTOCODE_ENABLE_CLONE=true OCTOCODE_SECURITY_CHECKS=true \
  node packages/octocode/out/octocode.js <tool> --input -
```

Add `mainGoal` and `reasoning` to each query. Continue a page only with the printed `{"cursor":"…"}`.

## Core tools

The core tools come first: a round that has not checked them is not ready, whatever else passed.

### Every tool, every time

| Check | Pass when |
|---|---|
| Answer | right for the selected question after all required pages; page 1 is useful and identifies how to continue |
| Identity | the next call can be built from the row (path and line, owner/repo and commitSha, number, id) |
| Lean | no request echo, no value under two names, no per-row constant, no static prose in results |
| More | the reply's top-level `next`, run until none is left, reaches all selected evidence; a row's `next.*` are branches; `related[]` is optional guidance, not a replacement for pagination |
| Paging | page 2 continues page 1 with no gap or repeat; a cursor with a changed request is refused |
| Incomplete | `isPartial`, `partialReasons`, coverage or skipped paths say what was not covered; a stated `total` is exact |
| Empty | an empty row says what was searched (`filesScanned`, coverage) and offers a lead |
| Errors | a bad call returns one cause line and a runnable example; a credential refusal names the fix |
| Redaction | a secret in a file shows as `[REDACTED-…]`, only the secret part, the same in every view |
| CLI = MCP | the same call gives the same answer on both surfaces, apart from cursors and timings |

For each registered tool, record the question IDs, operation/view coverage, valid and invalid
inputs, actual output, source oracle and both-surface result. Match CLI schema with a fresh MCP
`tools/list` under the same configuration; disabled opt-in tools are a configuration fact, not
a successful tool check. Verify the actual native executable behind both launchers. Capture
CLI stdout and stderr, plus structured JSON when testing field/continuation contracts. A schema
inspection or validation error alone cannot pass the positive execution path.
Also compare CLI `schema <tool> --view output` with the advertised MCP output reference at
`octocode://contract/output/<tool>` through `resources/list` and `resources/read`. Record formal
`outputSchema` presence separately; its absence does not mean output discovery is unavailable.

Do the common checks on local files without network access where appropriate. Distinguish an
empty answer from denied scope, partial coverage, missing credentials, provider refusal and a
failed process. For comparisons, use the same selected evidence and frozen inputs, count setup
and failed attempts, and separate fresh clients, persistent sessions and explicit provider caches.
End by stopping owned processes and removing only owned scratch/runtime assets.

### Sanity (after a build): review every tool the way an agent meets it

This is a review, not a checklist to tick. Start with each tool's [question set](questions/tools/README.md),
freeze the assigned source/ref, then add held-out questions when needed. Call, read and judge.
Rate each tool from **1 to 10** using [RATINGS.md](RATINGS.md), with one line of evidence per dimension.
The observations below feed those ratings; stress and agent-understanding coverage remain explicit.
Historical 1–5 results retain their original scale and are never silently doubled.

A large-window response from the same tool is a pagination control, not an independent correctness
oracle. Compare the selected facts with source/provider records before judging page reconstruction;
a page union can faithfully reconstruct an already lossy adapter result. Include package/symbol/ref
identity fields in that comparison, not just row counts.
Use the pinned binary on the CLI and on the MCP server (`node packages/octocode-mcp/dist/index.js`, stdio).

1. **Schema and description**: what an agent sees before its first call (`schema <tool>`, MCP `tools/list`).
   Does it say when to use this tool rather than its neighbour? Does it name every field needed to start a
   call, and nothing more? Are names aligned across tools, one name per concept? Are defaults, limits and
   refusals stated where they matter? Is there static guidance that responses repeat and that belongs here
   once?
2. **Input.** Write the call a first-time agent would write from the description alone, then near misses:
   a wrong type, a sibling tool's field, a missing field, an impossible combination. A valid call is
   accepted; a wrong one names the real field and an actionable repair, and starts no provider work.
   Unknown fields must not be silently ignored or accepted as aliases.
3. **Output** (`schema <tool> --view output`, then real answers). The answer comes first, with identity for
   the next call, one indication of completeness, and a way to get more. Check for: no echo, no value under
   two names, no static prose, counts that are true, a partial answer that says what it did not cover,
   redaction of only the secret part, and pages that join back to the source exactly once (try a small
   `responseChars` on a long file with non-ASCII lines).
4. **Workflows.** Start from a question, not a recipe, and let each response lead. Does every next call come
   from what the last one showed? Does the answer arrive in about the calls the workflow table expects?
   Where did you have to guess, re-read, loop or over-fetch?
5. **Agent understanding.** Give a fresh agent (a cheaper model is fine) only the MCP surface and a few
   real questions. Watch which tool it picks, which fields it uses, which leads it follows and what it
   misreads. Trace the cause: misleading schema/output and a runner's unsupported assumption are
   different findings. A capability check is not a blind research-agent experiment.
6. **Efficiency.** Characters and calls per answered question against the lean expert path in
   [COMPARISONS](COMPARISONS.md); first and repeat time; anything returned
   that the question did not need.
7. **Pagination.** Take every collection the tool returns (rows, files, matches, lines, patches, comments,
   nested lists, `rowPart` pieces) and walk it to the end with its own continuation. Use a small `limit` and
   a small `responseChars` so every kind of page boundary occurs. Joined, the pages hold every item exactly
   once, in order, with nothing truncated. A reply carries a top-level `next` exactly while more remains, and none
   at the true end. A cursor resumes in a fresh process and on the other surface. A
   cursor sent with an edited request is refused. Mixed batches page each row independently.
8. **Stress.** Push each tool toward its edges and judge how it behaves there. Try:
   - very large scopes (a monorepo root, a repository with thousands of files);
   - very large inputs: multi-MB files, a single line of several MB, minified code, deep nesting, binary
     files, many matches;
   - unusual text: non-UTF-8, CRLF, emoji and combining characters;
   - batches of five queries, and repeated calls back to back;
   - slow or failing providers: rate limits, timeouts, a cold language server.

   Good behavior here means the tool:
   - pages instead of truncating;
   - pauses with a resumable `next` instead of failing;
   - says what it did not cover;
   - keeps redaction to the secret part;
   - answers again in reasonable time and memory, and leaves no process behind.

   A refusal is fine when it names the limit and the way to narrow the call. A silent cut or a hang is a defect.

Also look at the surfaces: CLI and MCP give the same answer; an offline or tokenless run answers locally
and names the fix for GitHub without hanging; no `octocode` process outlives its call or client, except the
CLI's shared warm server: one `octocode serve` per binary build, workspace and configuration, which exits
after `IDLE_EXIT` (10 minutes) without a request, or on `octocode cache clear` or `auth login`/`logout`
([warm tool server](../octocode-native/README.md#warm-tool-server)).

Report one row per tool with the weighted 1–10 overall score, underlying dimensions, measured
coverage/confidence and question evidence, then each defect with its reproducing call. Explain
every deduction; a score below 8 names the material weakness. Good behavior may score well without
invented criticism. Apply the correctness/completeness cap in [RATINGS.md](RATINGS.md).

### End to end (before a release)

Behavior, judged by reading, on both surfaces. The per-tool sections below list the calls.

- [ ] Every `operation` and `view` value `schema <tool>` lists, at least once; a value without a visible
  effect is a defect.
- [ ] Every lead a response prints (`next.*`, `related[]`, hint calls) runs unchanged, is accepted and moves
  the question on; none returns to a call already made.
- [ ] Languages: one file per language the repository holds. `astSearch` `symbols` lists its declarations,
  a call pattern finds its calls, `localFetch` `view: symbols` agrees, and with an installed server
  `lspSearch` `definition` and `references` land on the right lines. CommonJS: references to the export of
  `test-utils/fixtures/lsp/cjs-noconfig/a.cjs` find the `require` use in `b.cjs`, not unrelated text.
- [ ] Every ecosystem `artifactSearch` and `securityCheck` accept agrees with the registry's or OSV's own
  answer; `securityCheck` without a token gives the same verdicts and discloses a rate limit instead of
  waiting it out.
- [ ] Provider fields: for a repository, a package and an advisory, `view: details` beside the raw answer
  (`gh api`, registry JSON, OSV). A field the provider sends that the tool neither shows nor states as left
  out is a defect.
- [ ] Provider cache controls: use owned mutable fixtures for repository, registry and OSV answers.
  Preserve repeated headers, Vary: *, no-store and revalidation requirements through every cache layer;
  update policy on 304 and reject old persisted entries that lack the required metadata. Compare cold,
  warm and restarted clients. Identical concurrent requests may share a usable answer, but forbidden
  reuse, changed request identities, body bounds and cancellation must still reach the correct outcome.
  Count all physical requests and completed evidence before claiming a speed or cost improvement.
- [ ] Successful output: optional hints appear only for empty/error recovery. Coverage warnings,
  source identity, required reads and every continuation survive concise output and transformed views.
- [ ] clasify: a repeat call makes a new billed request and reports its usage; an edited source or changed
  question is judged anew; a refused URL makes no request; a warning names the quiet units a summary leaves
  out and `next.quiet` lists them. Answers are judgments, so judge the framing, not the number: is each question one standalone
  fact, does the judge see the hit line marked and its enclosing context, and does `next.read` lead to the
  deciding lines? A surprising answer is first a question about our wording or context.
- [ ] At two refs, compare `git rev-parse --git-common-dir`, verify ancestry against the origin and repeat the first request after the second. Check the remote tag and that a local tag survives.
- [ ] ghCloneRepo: other `paths` give another checkout; a refresh over a local edit
  refuses (`checkoutDirty`) and keeps the edit.
- [ ] Every workflow below, in its number of calls.
- [ ] The same calls on the published release (`npx -y octocode@latest`): any answer that got worse names
  its call.
- [ ] Settings: each of `OCTOCODE_BETA`, `OCTOCODE_ENABLE_CLONE`, `OCTOCODE_SECURITY_CHECKS`, a classification
  key, `TOOLS_TO_RUN` and `DISABLE_TOOLS` changes exactly its tools in MCP `tools/list` (the CLI lists all), and
  so do the legacy `ENABLE_LOCAL` and `ENABLE_CLONE`;
  listing makes no provider request; a call to a missing key's `clasify` gives `missingConfiguration`.
- [ ] Launcher (`node packages/octocode/out/octocode.js`): argv, stdin (`--input -`), the exit code, SIGTERM
  (signal death as 128+N) and `--version` pass through to the native binary; with the platform package
  missing it names the package and fetches the pinned one; `config view` refuses a request with a foreign
  `Host` or no session token.
- [ ] Plugins: the packed Claude plugin in a temporary `CLAUDE_CONFIG_DIR` and the Codex plugin in a temporary
  `CODEX_HOME`, no real credentials: the installed manifest and skills are complete, and the MCP server reads
  only the project root it starts in.
- [ ] Credentials: the [packaged auth checks](../octocode-native/docs/GITHUB_AUTH_TESTS.md#packaged-checks).

- [ ] The [security checks](#security) and the areas owned elsewhere ([Areas](#areas)).

### Per tool

#### localSearch
- Calls: content search in a folder; `view: files`, `filesWithout`, `countMatches` with `limit` and
  `sort:{"by":"matchCount","order":"desc"}` (sorted by count); `matchOnly` with a regex; `contextLines`; `wholeWord`; `include`/`exclude`; a term that only matches
  as a regex (expect a hint).
- Look for: hits grouped by file with line numbers, in walk (name) order; on a content page that has more, no `pagination.total` and no `matchCount` are stated; a hit in a line over 4 KB shows a window with its column.
  Follow the related `localFetch` call to read the whole line. Expect one range per clipped line, grouped by file,
  and no full-line read for ordinary hits.
  Empty rows carry `files: []` and `filesScanned`, no zero counters.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#localsearch).

#### localFetch
- Calls: whole file (page through); `ranges`; `matchString` + `block: true` on a short and a 600-line
  function; `view: symbols`, `compact`; a minified file with and without `matchString`.
- Look for: the exact declaration on page 1 and a full first page for a long one. When a long matched line exceeds
  the response window, the reply shows the match's byte window and a `related` read of the full line.
  Follow that call and any response cursors to reconstruct the entire line. An explicitly selected long line that
  exceeds the response window has its own byte-window cursor.
  A credential file name is refused.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#localfetch).

#### structureSearch
- Calls: `operation: tree` (names only); `maxDepth`; `operation: files` with `sort` by `bytes`, `totalLines`,
  `modifiedAt` (no `order`: largest or newest first); `nameRegex` of a parent and of its child; `view: details`;
  a ranked row's path pasted into `localFetch`.
- Look for: every directory has its own row; a metric sort shows that one value per row; the parent and child
  filters give different listings.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#structuresearch).

#### astSearch
- Calls: `operation: symbols` on a file and a folder; `match` with a `rule` (kind + has/inside); a bare call
  pattern `f($$$A)` in Rust where only `x.f(…)` exists; `syntaxTree`.
- Look for: matches only of the shape asked (no comments or strings); an empty bare pattern says how many
  receiver calls exist and offers the rule call.
- On a line with two different matching calls, each hit gives its 1-based UTF-16 column, including when a page boundary separates the hits.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#astsearch).

#### lspSearch
- Calls: `references`, `definition`, `callers`, `hover`, `documentSymbols`, `workspaceSymbol` on a TS and a
  Rust symbol (needs `line`), cold and repeat.
- Look for: rows with roles (declaration, import, call); `coverage.state complete` unless something real is
  missing; one cold-start warning, not three.
- For `import { greet as hello }`, references mark both tokens as imports. Their `importRole` values are `importedName` and `localAlias`. A call to `hello()` still has the call role.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#lspsearch).

#### graphBuild and graphQuery
- Calls: build a package folder; `stats` (summary, then `view: details`); `hubs`; `cycles`; `issues` with
  detectors; `find`; a second build of an unchanged folder (reuses the snapshot).
- Look for: the receipt carries linkage, not the call breakdown; stats summary is short and leads to details;
  issue rows state shared controls once; findings are marked as hypotheses.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#graphbuild-and-graphquery).

#### clasify
- Calls: `evaluate` with a small `content` and 3 questions; `source` as a large file; `source` as another
  tool's query (`{tool: "localSearch", query: {…}}`); `content` array with `perItem: true`; `sections` over a
  folder glob walked through the reply's `next`; an image with noul + choice + box; a choice question whose answer is in one
  section only.
- Look for: one answer per question with the deciding `lines`; coverage counts; units keyed by path:line or
  number; the one knowing section wins over `unknown`; `view: details` adds probabilities and confidence;
  `usage` tokens and request count.
- Fixed cases (rerun after any change to clasify, its description or how rows share requests; each passes only
  on the stated answer):
  - Abstain: 8 short support messages, 4 fitting one of 4 labels and 4 fitting none, one `choice` with
    `unknown`. Pass: all 8 right.
  - Pick one of many: about 70 file paths as `criteria` plus `none`, three descriptions that each match one
    file and one that matches none. Pass: the three files chosen at confidence 0.9 or above; the fourth below 0.5.
  - Extract a value: a manifest, candidates from a version-shaped search plus `none`, one value the file
    states and one it does not. Pass: the stated value chosen; `none` for the other.
  - Rows share requests: 48 paths from `structureSearch` as a `source`, one yes/no question. Pass: one request;
    every file whose name states the topic ranks above every file whose name does not.
  - Collapsed log: an 8,000-line access log with one 500 and two 404 lines, `collapse: true`. Pass: one
    request; the 500 keyed by its own line number; the 404 keyed by its first line.
  - Scout budget. With a fake provider and an allowance of two requests, evaluate a source that returns
    one unit per page under `maxRequests: 1`. Follow the billed page continuation once. Pass when both
    pages report one new `usage.calls`, a saved-answer replay reports `replayed: true` and adds no
    provider requests, and the caller stops before a third paid page.
  - Big file in parts: a log over the provider window, no `maxRequests`. Pass: one call; each planted failure
    in the part whose lines hold it; quiet parts counted in `coverage.quiet`.
  - Image as a source: a PNG as `source` with a question that says "the image". Pass: one request, and its
    input tokens match the same question worded without that word.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#clasify).

#### ghSearchRepo
- Calls: `search` with keywords, filters, `sort {by: stars | forks | updatedAt | relevance}`; `get`; `list` for
  an owner; `fields` for moved columns.
- Look for: rows of owner/repo, stars, description; the sorted value shown; moved fields back via `fields`; an
  old sort name refused with the new one named.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#ghsearchrepo).

#### ghSearchCode
- Calls: keywords in a repo; `path`; `fields: ["path"]`; `matchIn: ["path"]`; two pages.
- Look for: matched lines per file; a renamed repository followed and named; page 2 fast and without repeats.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#ghsearchcode).

#### ghStructure
- Calls: `tree` of a folder, `maxDepth`, `include` glob (`*.ext` under `path`, any depth), `sort {by: bytes}`; `refs`,
  `refs` with `refKinds: ["tags"]` and `limit`; `languages`; `materialize` on a few files.
- Look for: sizes ranked over the whole listing; the repository's own release tags first; tags only when asked.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#ghstructure).

#### ghGetFileContent
- Calls: a small file; `ranges`; `matchString` + `block` on a long function; `view: symbols`, `compact`; a
  minified file with `matchString`; a path at a tag `ref`; a directory path (expect a clear error).
- Look for: the same as localFetch; `commitSha` on the first page.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#ghgetfilecontent).

#### ghSearchHistory
- Calls: `commit` for a path; `pullRequest` with keywords, `filters: {isMerged: true}`, `sort {by:
  commentsCount}`; `issue` with state; `fields`; two pages.
- Look for: sha/number, date, headline by default; author and labels via `fields`; the sorted value shown.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#ghsearchhistory-and-ghgethistoryitem).

#### ghGetHistoryItem
- Calls: a PR summary; PR `sections: ["files"]`, `["reviews"]`, `["patches"]` with `include`; a commit with
  `sections: ["patches"]`; `compare` of two tags with one file; an issue with comments.
- Look for: a section read carries number, state and the commit ids, then the section; the compare summary
  pages commits apart; patch lines carry line numbers.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#ghsearchhistory-and-ghgethistoryitem).

#### artifactSearch
- Calls: `resolve` npm with a version, PyPI latest, crates latest; `search` npm; `view: details`.
- Look for: name, version, owner/repo, directory, ref (commit) in the summary; dependencies, licence, engines
  in details; the summary's lead runs to details.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#artifactsearch).

#### securityCheck
- Calls: `package` with a vulnerable version (`ecosystem` as `artifactSearch` returned it); `repository` by
  `owner` + `repo`; `commit` by `ref` (one known, one unknown); `url` as a purl;
  `advisory` by id; each with `view: details`.
- Look for: `affected`, `upgradeTo`, advisory count, highest severity, one link; an unknown commit says that 0
  proves nothing; details lists every advisory with ranges and fixes.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#securitycheck).

#### ghCloneRepo
- Calls: a small repository; a sparse `paths` clone; then localSearch inside the returned location.
- Look for: the location and commit stated; a repeat reuses the clone; local tools work on it.
- Compare: the expert command and what "better" means are in [COMPARISONS.md](COMPARISONS.md#ghclonerepo).

### Workflows (each must finish in the same number of calls as last round)

| Workflow | Calls |
|---|---|
| Search a symbol, read its body | ghSearchCode → ghGetFileContent `matchString` + `block` |
| Folder, then an outline | ghStructure tree → ghGetFileContent `view: symbols` |
| A file's history, then the newest patch | ghSearchHistory `commit` with `path` → ghGetHistoryItem `patches` with `include` |
| A merged PR, then its files and reviews | ghSearchHistory `pullRequest` → ghGetHistoryItem `sections` |
| A tag, then one file's change between releases | ghStructure `refs` → ghGetHistoryItem `compare` |
| A package version, then its source at that release | artifactSearch `resolve` → ghGetFileContent at the returned `ref` |
| Is this version affected | securityCheck `package` → `view: details` only if the list is needed |
| Many searches in one repository | ghCloneRepo → localSearch ×3 |
| Local: find, then read | localSearch → localFetch `matchString` + `block` |
| Local: who calls it | localSearch → lspSearch `callers` at the hit's line |
| Local: what is unused | graphBuild → graphQuery `issues` |
| Large unread evidence | clasify with `source` (a file, or a tool query) → read only the deciding lines |

## Security

The user-facing rules are in the [security guide](../../docs/SECURITY.md); this section is how a round checks
that the tools behave by them. Work in a scratch workspace with planted, fake secrets (never real ones),
point anything that would reach a network host at a loopback server, and check each item on the CLI and the
MCP server. Pass means the behavior below; anything else is a defect with the call that shows it.

| Check | Provoke it with | Pass when |
|---|---|---|
| **Partial redaction** | a fake token, key and password inside ordinary source lines; a line of several MB that holds one | only the secret part becomes a `[REDACTED-…]` marker; the rest of the line stays; the same marker appears in every view (`localSearch` hits, `matchOnly`, `localFetch` whole, `ranges`, `compact`, `symbols`, a byte window) and on every page, with offsets counted on the redacted text |
| **Large-file masking** | a multi-MB file and a minified one-line file with secrets at the start, middle and end, read in small `responseChars` pages | each page is redacted as it streams; no page or window boundary splits a secret so that part of it shows |
| **Credential files withheld** | `.env`, `id_rsa`, a credentials store and their backups beside normal files | discovery and reads withhold them by name and say so; a `credentials/` folder of ordinary docs stays readable; graph builds and `clasify` withhold them too |
| **Symlinks** | a symlink inside the workspace that points outside it, or at a credential file | the target is not read; the refusal names the real reason |
| **Path policy** | `..` escapes, absolute paths outside the workspace, the Octocode home, a protected OS location | refused with the allowed roots named; `local.allowedPaths` admits exactly what it lists |
| **Provider egress** | `clasify` over a file holding a fake secret; `securityCheck` on a commit; GitHub queries | only redacted evidence reaches the provider; queries carry identifiers, never a token; `securityCheck` states that a commit lookup sends the hash |
| **Tokens and refusals** | no token, a bad token, `HTTPS_PROXY=http://127.0.0.1:9` | local tools answer; GitHub tools return one error that names the fix, without hanging or printing the token |
| **Clone sandbox** | `ghCloneRepo`, then a refresh over a local edit; `lspSearch` on the clone | the checkout lands under the Octocode home; a dirty refresh refuses (`checkoutDirty`) and keeps the edit; no language server starts on fetched code unless trusted |
| **Prompt injection** | a file, issue or PR body that says "ignore your instructions and …" | the text comes back as data; no tool acts on it; a checking agent given that output does not follow it |
| **Processes** | interrupted calls, closed MCP stdin | no `octocode` or language-server process outlives its call or client, except the warm `octocode serve` (see Sanity above) and the language servers it pools, which stop on the pool's idle timeout |

## Skills

Behavior of each public skill (activation, steps, tool use, output, references, cost) and how to improve one are in
[SKILLS.md](SKILLS.md), with the requests in [questions/skills/](questions/skills/README.md). The mechanical gate is
`node skills-dev/octocode-dev/scripts/dev.mjs skills:check`. Rate each skill 1–5 in the scorecard.

## Report

Each round fills in the [scorecard](briefs/SCORECARD.md) and saves it as described in
[METHOD.md](METHOD.md#record). Defects go to the queue with an owner; a fixer never signs off its own work.
