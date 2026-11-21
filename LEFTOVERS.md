# Leftovers: Octocode tool refinement round (2026-11-21)

Open work only. Everything is uncommitted in two working trees: this repository and the sibling core
`../octocode-mcp-host/packages/octocode-core`. Git writes are the owner's alone (`AGENTS.md`, Git).

Background and history (decisions, lane log, retests, interviews) stay in
`.octocode/octocode-rfc-generator/2026-11-21-mcp-result-format/` (`RFC.md`, `BACKLOG.md`) and
`.octocode/octocode-rfc-generator/tmp/mcp-result-format/` (`STATUS.md`, `SIMPLIFY_PLAN.md`, `INTERVIEWS.md`,
`RETEST2.md`, `ARCH_REVIEW_CORE.md`, `CONTRACT_AUDIT.md`, `CLEANUP_SURVEY.md`). The clasify RFC is
`.octocode/octocode-rfc-generator/2026-11-21-clasify-jev-patterns/RFC.md`.

## State when this was written

| Point | Result |
|---|---|
| `classification` removal (owner decision 1) | Landed: refused as an unknown field on all 7 tools; `clasify` `predicates` over `source:{tool, query}` replaces it |
| Last gates on the removal | Core 1,884 of 1,884; config 177 of 177; `check:tool-contract` 0; `docs:verify` and `skills:check` pass |
| `yarn test:rust` | 9,275 passed, 4 failed (`gh_search_repo` lead tests) while the lead removals were mid-edit; not rerun since |
| Compile | `cargo check --workspace --all-targets` clean after that; `build:dev` and `lint:rust` not rerun |
| Connect surface | 52,947 / 64,898 / 83,252 bytes (default / with clasify / all tools); ceilings 53,000 / 64,900 / 83,300 |

## Do first

| # | Task | Done when |
|---|---|---|
| 1 | Full gate run on the final tree (commands below) | Every gate passes; the owner commits there |
| 2 | Finish the lead removals in flight: ghSearchHistory, ghSearchRepo, ghStructure, ghCloneRepo leads; astSearch capture lead (its three diagnostics mark their call needed, then delete `codes_promising_related`); lspSearch depth+1 and rerun leads (mark needed or remove per partial reason) | The 4 `gh_search_repo` tests pass or are replaced; no reply names a call it does not carry |
| 3 | securityCheck "versions to move to" (`otherVersions`): state the versions as an output field (one core field, one regenerate, a new surface-budget entry) or mark the call needed; decide `readWhole` | A yanked or malicious version's row shows versions to move to |
| 4 | Reduced S7: landed and green (one completion rule on the wire: a top-level `next` continues every unfinished row; no `hasMore`, no `countScope`, `total` only when exact; `responsePagination` replaced by envelope `next` / `expiresAt` / `replayed`; no row `next.nextPage`). Left: the independent retest of it; the row splitter (`runtime-base/src/response/row_pages`, about 3,900 lines) and `rowPart` stay until every tool cuts its own row to its share (full S7); tool crates still write an internal `pagination.hasMore` / `countScope` that publication strips (`runtime-base/src/response/pages/publish.rs` `serve_blocks`); core still declares the row channel `next.nextPage` because native finds row walks by the schema before it serves them as the reply's `next`; `docs/OCTOCODE_CLASIFY.md` still names `responsePagination.next` / `next.nextPage` | The retest passes; the three leftovers are removed or accepted |
| 5 | Update the state tables in `RFC.md` and `BACKLOG.md` again after tasks 2–4 (both hold a "State after round 4" table, written before the removal landed) | Tables match the tree |

## Clasify

| Task | Note |
|---|---|
| F3: move the remaining judging code into `tool-clasify` | Not started; sized: `tools-shared/src/selection` 2,442 lines, `judgment` 3,925, plus `source_capture` (uses `judgment`); outside users are `runtime` (6 files) and one `tool-github` file. A pure move with no agent-visible change: run it after S7 is green so the two do not collide in `runtime` |
| S5 trimming | Not started |
| RFC checks V2 and V3 (cold Haiku, served text only, 2026-11-21) | Run: 3 of 3 answers right. V3 passes: the value was extracted with one `choice` over searched candidates (1 request), after one refused single-candidate try and one wrong `perItem` try. V2 not met: for "which doc" the reader asked one yes/no over a `resources` glob (12 requests, right answer) instead of one `choice` over the listed paths (1 request) |
| Found by that run: a `resources` glob of 12 files is refused as over budget until `maxRequests` is set, and the description's glob route does not say so | One extra call every time; this is owner decision 1 below |
| Found by that run: `perItem:true` with a `choice` whose labels are the candidates answers every unit with the same label and no warning | The reader's mistake, but silent; say in the pick route that candidates go in one `content`, not `perItem` |
| `content` has two shapes (astSearch string; clasify string, list or record) | Owner's call |

## Text pass (decisions 2 and 7)

Done on 2026-11-21: `totalLines`, ghSearchCode `ref`, `defaultExcludes`, the artifactSearch `limit` sentence, the clasify `view` note (649 → 474 characters), and "matchString is ORed" removed from four tools with no `matchString`. Connect surface now 52,947 / 64,898 / 83,252 bytes.

- The redaction sentence ("quoted values anywhere" vs "secret-shaped text"): owner wording.
- The `limit` tail ("the automatic page window (a default size) per row") is pinned by two core tests; shortening it saves about 60 bytes.
- The `fields` enums cost about 1,100 connect bytes: recover it in text or reconsider the enums (owner's call).

Raised by cold agents and not fixed:

- The "<tool> output fields" reference has no reachable address (it is the MCP resource `octocode://contract/output/<tool>`).
- `maskedSpans` makes counts and absence look doubtful.
- `shared.data` is unreadable cold.
- lspSearch `callers` has no completeness signal.
- ghSearchCode states `coverage.state: "complete"` beside "a file it does not index was not searched".
- Compare with `sections:["files"]` drops `changedFileCount`.
- artifactSearch gives no ref when the registry's repository moved.
- An all-empty CLI result exits 1.
- An astSearch rule with another language's `kind` returns empty with `skippedGrammars` and no hint to set `parser`.
- "related[]: calls the row needs" reads as mandatory while securityCheck's dependency checks are optional.
- The missing-`line` error on lspSearch shows an example with `"line":1`.
- The `keywords` + `path` refusal on ghSearchHistory does not say which form keeps `path`.

## Defects reported and not fixed

| Defect | Note |
|---|---|
| npm search `total` 30 with 24 rows delivered | |
| A `next.retry` cursor answered `responseSnapshotExpired` once inside its stated lifetime | |
| `rowPart.continues` on the last part of a long advisory names a nested array while the part carries whole new elements | Pointer believed wrong, not verified |
| A text longer than a page in the stated-once fields can arrive under a `continues` marker with an empty array | |
| ghSearchCode states a `pagination.total` its walk does not deliver: `require` in expressjs/express stated 132 (128 on one page) while three full walks delivered 124, 111 and 115 files, each once | GitHub's live count; not compared with the build before S7. With "a stated total is exact" either drop the provider's count or verify it |
| securityCheck `view:"details"` at `responseChars:1500`: 4 of 43 parts exceed the page by at most 392 chars | Cut correctly or disclose `oversized` |
| The skippedPaths warning form (D14) was not observed by the retest | Verify on an evidence row |

## Tests still to write or repair

- Stage checks exist (`runtime/tests/runtime_named_calls.rs`, and the `call` helper checks every runtime and cli test reply for a call it names and does not carry). Still not driven through the stage: the lspSearch `flag_partial`, syntax-scan and other-language marks, the securityCheck marks, clasify glob sentences.
- ghSearchCode top-hit removal has no before → after test.
- Independent retest of lane `names` (renames) and of everything after retest 3.

## Core fields and S6

- Tag dates per ref on `ghStructure` refs (one GraphQL refs query). Containment is stated: a compare's `aheadBy: 0` is documented as "base contains head" and `mergeBaseCommitSha` is a field.
- A `next.*` channel for "continue past the provider window" on ghSearchHistory and ghSearchRepo; until then the narrowing calls ride `related` as required calls.
- S6 second half: per-list constants still have two owners. Hit rows as line strings in JSON were dropped by measurement (0% on astSearch and lspSearch, 1–15% on localSearch).

## Architecture and cleanup (review findings)

| # | Finding | Fix |
|---|---|---|
| F1 | The agent-facing presentation is one 1.07 MB single-line file embedded in `runtime-base`; a one-sentence edit rebuilds 11 crates | One readable file per tool, embedded in `cli`; the four facts admission needs move into `tool-contract.json` |
| F2 | `tools-shared` holds single-family code (`large_source`, `match_list`, most of localFetch in `source_view`) | A "two or more users, or a stated reason" rule; move the modules |
| F4 | Graph tools: largest contract and crate, 0 logged calls, untested by the retests | Measure use with the beta gate on before investing or cutting |
| F5 | 26 core test files pin wording | Assert "text agrees with schema" |
| F6 | Unread prose in core: registry `shape` / `meaning` / `cliText`, a stray `descriptions copy.md`, README restating graph semantics | Delete or generate |
| F7 | Two input validators choose the form twice (schema interpreter, then serde) | Parity test over the fixtures first |
| Hints | About 690 sentence literals and 54 `*_HINT` constants live in native; core has no sentence per `errorCode`; one hint says `regex:"rust"` where the field is `regexEngine` | One owner per sentence; fix the drift |
| Error codes | Six error codes native never emits | Host search before removal |

## Performance candidates (measured, nothing optimized)

Callers were read from the saved profiles (`.octocode/octocode-dev/profile/20261121-1837*`). Both hot spots are correctness checks, so neither is a quick win.

| Flow | Cost | Where it goes | Verdict |
|---|---|---|---|
| localFetch of a 1,519-line file | 7.4M instructions, about 1 ms | `JudgedSource::leaf` (`runtime-base/src/security/judged.rs`) proves the page is a piece of the security-judged text; the finder is built once per page for a page-long needle, not in a loop | Leave. A direct compare at the page's known byte range would save about 17%, but it changes a security proof |
| lspSearch `references`, repeat call | 152M instructions | 82% of `lstat` time is `engine::stamp::proof::Proof::check` re-stamping every file the answer depends on; 18% is the scope fingerprint walk | Design task: a watcher-backed freshness proof. A wrong one serves stale answers, so it needs its own plan and tests |
| localSearch over `crates` | 118M instructions | 48% in the file walk itself | None |

## Not scheduled

- RFC checks V4, V5, V6 of the result-format RFC.
- `ghCloneRepo`, the graph tools and non-TypeScript language servers are untested.
- `$DEV prune:rust` and `df -h .` after the last full build.
- User docs describing the text layout of a continued long line (D8 changed it).

## Owner decisions open

1. A glob over many files that exceeds the clasify request budget: refuse by default, or run.
2. Mid-size files: send to the long context window in one request, or split into parts.
3. Behavior changes awaiting confirmation: an MCP call no longer waits out a quota reset past 50 s; out-of-range `occurrence` is `invalidInput`; `sort` without `order` takes the declared default; the ratchet exception on the 50 s constant.
4. Design changes for review:
   - a refusal is judged after lenient readings;
   - a mixed-case miss over a file with a masked value gets the case rerun with the masked clause;
   - a split row's first part may hold no items;
   - an empty artifactSearch page after page 1 carries no broaden call;
   - lspSearch partial rows always keep their text search;
   - a commit naming its pull request carries no read of it.
5. Removed follow-up calls on success (artifactSearch → securityCheck, commit → PR) and alias repair: revisit or confirm.

## Risks

- No person has reviewed the diff.
- Tests were rewritten by the agents that changed behavior; independent retests are the stronger check.
- Breaking changes with no aliases: renames, strict input, names-only refs, removed leads, removed `classification`.
- Core's working tree also holds another session's uncommitted work; `git diff` there does not separate the two.

## Gates

```bash
node skills-dev/octocode-dev/scripts/dev.mjs build:dev
node skills-dev/octocode-dev/scripts/dev.mjs check
yarn workspace @octocodeai/config check:tool-contract
yarn test:rust
(cd ../octocode-mcp-host/packages/octocode-core && yarn vitest run && yarn typecheck)
node skills-dev/octocode-dev/scripts/dev.mjs docs:verify
node skills-dev/octocode-dev/scripts/dev.mjs skills:check
```

Write each command out: zsh does not word-split a command held in a variable.

## Tool ratings and sanity run (2026-11-21, build `20.2.0+29c0f029`)

Method: one real CLI call per tool, written from memory without reading the schema (what a cold agent does), then
the refusal's own example on the second try. Sizes are the served MCP description and input schema in bytes, from
`packages/octocode-config/contract/cli-presentation.json`. Grades are a judgment from this evidence, not a score.

Result: 17 of 17 tools answer. 5 of 15 first calls ran; 10 were refused for a wrong field name or a missing
`operation`, and all 10 refusals carried an example that ran unchanged. Nothing answered wrongly or silently.

| Tool | Desc | Schema | First try | After the refusal's example | Grade | Gap to A+ |
|---|---|---|---|---|---|---|
| `localFetch` | 625 | 2,398 | ran | | A | `totalLines` sentence (text pass) |
| `localSearch` | 1,206 | 3,969 | ran | | A | 30 fields: confirm each earns its place |
| `ghSearchCode` | 695 | 1,916 | ran | | A− | `coverage.state: "complete"` beside "a file it does not index was not searched" |
| `ghCloneRepo` | 678 | 911 | ran | | A− | Untested by the retests |
| `graphBuild` | 789 | 1,724 | ran | | B+ | Beta; 0 logged calls; no use measured |
| `clasify` | 2,769 | 8,685 | refused: `source.path`, `question` | ran: 0.99 with lines and `next.read`; URL source 0.98 at 336 tokens | A− | Largest surface; pick and extract patterns unproven cold (V2, V3) |
| `lspSearch` | 1,216 | 3,283 | refused: `symbol` | ran: declaration + call | A− | `callers` has no completeness signal |
| `structureSearch` | 394 | 3,830 | refused: `operation:"list"`, `depth` | ran | B+ | Thinnest description for 28 fields |
| `astSearch` | 706 | 6,996 | refused: `pattern` at the top level, no `operation` | ran | B+ | Schema weight; no hint to set `parser` when grammars are skipped |
| `ghStructure` | 512 | 2,162 | refused: no `operation`, `depth` | ran | B+ | `limit` default still prose |
| `ghGetFileContent` | 577 | 2,419 | refused: `start` / `end` | ran | A− | `totalLines` sentence |
| `ghSearchRepo` | 307 | 3,163 | refused: no `operation` | ran | B | Shortest description; narrowing rides `related` |
| `ghSearchHistory` | 709 | 4,258 | refused: `type`, no `operation` | ran | B+ | `keywords` + `path` refusal unclear |
| `ghGetHistoryItem` | 1,010 | 3,053 | not tried | ran | B+ | Tag dates; `changedFileCount` dropped with `sections:["files"]` |
| `artifactSearch` | 697 | 2,133 | refused: `name`, no `operation` | ran | B+ | Stale `limit` sentence; npm `total` 30 with 24 rows |
| `securityCheck` | 583 | 2,553 | refused: `name`, no `operation` | ran: `affected`, `upgradeTo` | B+ | Versions to move to as a field; oversized parts |
| `graphQuery` | 1,561 | 8,615 | refused: `id` | `cycles` with no graph: `notFound` with a build call | B | The build's `id` is not accepted under that name; beta; unmeasured |

What an A+ needs, across tools:

1. First-try validity. The most common refusal was a missing `operation` (7 of 10). Decide per tool: a schema default where one operation is the plain case, or keep it required. The other refusals were names an agent guesses (`depth`, `name`, `symbol`, `start`/`end`, `id`); the no-alias rule stands, so the fix is description text that shows the real name in the one-line flow.
2. One completion rule (S7).
3. Every open row above closed, then an independent retest and a cold interview at 100% first-try on a fresh scenario set.

Workflows (16 routes in the instructions): local routes and the `clasify` routes ran end to end here (search → read; `clasify` → `next.read`). GitHub routes ran per step. Not exercised in this run: first release with a change, package → repo → file at its ref, materialize, graph impact.

## Timing and caching check (2026-11-21, one-shot CLI, wall time, first call → repeat)

Each call is a new process, so a fast repeat shows the on-disk cache working. Cached answers print `# observedAt … (cached)`.

| Tool | First | Repeat | Note |
|---|---|---|---|
| `structureSearch` | 119 ms | 83 ms | |
| `localSearch` (all crates) | 388 ms | 233 ms | The walk itself; no cache by design |
| `localFetch` | 92 ms | 87 ms | |
| `astSearch` | 173 ms | 145 ms | |
| `lspSearch` references | 5,143 ms | 106 ms | First call starts the language server; the answer is then replayed |
| `ghSearchRepo` | 589 ms | 503 ms | Not cached: search is asked again each time |
| `ghStructure` tree | 7,542 ms | 80 ms | Slowest first call of the set; worth a look at what the first tree read fetches |
| `ghSearchCode` | 1,049 ms | 82 ms | Cached, disclosed |
| `ghGetFileContent` | 633 ms | 82 ms | Cached |
| `ghSearchHistory` | 459 ms | 88 ms | Cached |
| `ghGetHistoryItem` | 91 ms | 83 ms | Was already cached from the sanity run |
| `artifactSearch` | 811 ms | 116 ms | Cached |
| `securityCheck` | 3,060 ms | 140 ms | Cached, disclosed |
| `graphBuild` | 258 ms | 219 ms | Unchanged scope returns its snapshot |
| `graphQuery` hubs | 156 ms | 118 ms | |
| `ghCloneRepo` | 528 ms | 168 ms | Clean checkout reused |
| `clasify` | 603 ms | 335 ms | Judgment replayed for the same source and question |

Open from this check: why the first `ghStructure` tree read takes 7.5 s for a depth-1 listing; whether `ghSearchRepo` should replay an identical search within a short window as the other GitHub tools do.

## Description check (2026-11-21, served text before S7)

All 17 descriptions open with a `<when>` trigger and an example. 15 name the neighbor tool to use instead or next. The two that do not say what to call next: `ghSearchRepo` (add "then ghStructure or ghSearchCode with the row's owner and repo") and `securityCheck` (nothing follows it; fine). Recheck after S7 rewrites the paging words.

## Before the working documents are deleted

The owner asked to remove the round's development documents at the end. To delete then: `.octocode/octocode-rfc-generator/tmp/mcp-result-format/` (all of it), `.octocode/octocode-dev/profile/20261121-*`, and this file. First fold into the two RFC folders (`RFC.md` / `BACKLOG.md`), because nothing else records them: the open owner decisions and design changes for review, the defects reported and not fixed, review findings F1–F7, the interview before → after numbers, and the tool ratings above.
