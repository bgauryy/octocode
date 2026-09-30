# Eval questions

The 20 questions used by the agent-vs-agent eval (10 GitHub, 10 local). They were selected from the question pools of earlier benchmark campaigns; the wording is unchanged except that corpus placeholders are resolved to absolute clone paths. [questions.json](questions.json) is the machine-readable copy that the harness reads.

No answer keys are kept in this package: the judge establishes ground truth for each question itself.

## Sources

- **GitHub questions** come from the shared 30-question GitHub pool that lived in `compare/github-questions/Q1..Q30.md` (removed with the old campaigns; it remains in git history). They ask about live repositories and default branches, so correct answers can drift over time.
- **Local questions** come from the Terra v3 public suite (`compare/terra-v3`, cases `p01`–`p20`) and the advanced-research-v1 pool (`A01`–`A12`), both also removed. Both pools share one corpus, pinned below. Terra cases that named a specific tool (for example "Using frozen Pyright") were skipped so that neither arm is favored by the wording.

Local corpus, cloned by [octocode-local-testing/repos/README.md](../../../octocode-local-testing/repos/README.md):

| Repository | Commit | Clone path |
|---|---|---|
| langchain-ai/langchain | `67ee6cb63dd9ae7f3a4dfedc3095652bce15a125` | `octocode-local-testing/repos/langchain` |
| vercel/next.js | `d155ba9ebfffe4742efefda8d68c2e0e8e490924` | `octocode-local-testing/repos/nextjs` |

## Index

| ID | Surface | Type | Source | Repositories |
|---|---|---|---|---|
| [G01](#g01) | github | locate | Q1 | GitHub (live) |
| [G02](#g02) | github | history | Q3 | GitHub (live) |
| [G03](#g03) | github | history | Q5 | GitHub (live) |
| [G04](#g04) | github | trace | Q6 | GitHub (live) |
| [G05](#g05) | github | history | Q13 | GitHub (live) |
| [G06](#g06) | github | semantic | Q17 | GitHub (live) |
| [G07](#g07) | github | trace | Q19 | GitHub (live) |
| [G08](#g08) | github | history | Q22 | GitHub (live) |
| [G09](#g09) | github | multi-hop | Q24 | GitHub (live) |
| [G10](#g10) | github | semantic | Q29 | GitHub (live) |
| [L01](#l01) | local | locate | p01-langchain-runnable-config-declarations | langchain-ai/langchain |
| [L02](#l02) | local | locate | p08-next-react-create-context | vercel/next.js |
| [L03](#l03) | local | trace | p18-langchain-runnable-config-flow | langchain-ai/langchain |
| [L04](#l04) | local | trace | p19-next-config-loading-flow | vercel/next.js |
| [L05](#l05) | local | multi-hop | p20-cross-repo-tracing-extension-points | langchain-ai/langchain, vercel/next.js |
| [L06](#l06) | local | multi-hop | A02 — Configuration cache collisions and observable work | vercel/next.js |
| [L07](#l07) | local | multi-hop | A10 — Route parameter encoding and optional catch-all semantics | vercel/next.js |
| [L08](#l08) | local | semantic | A03 — Retry only the failed batch members | langchain-ai/langchain |
| [L09](#l09) | local | semantic | A04 — Fallback boundaries and exception injection | langchain-ai/langchain |
| [L10](#l10) | local | semantic | A06 — Streaming versus invoking a composed pipeline | langchain-ai/langchain |

Types: **locate** finds a definition or an exact set; **trace** follows a call or data path; **history** reads commits, PRs, issues or comparisons; **multi-hop** chains several lookups or repositories; **semantic** explains how a mechanism behaves.

## G01

- Source: `compare/github-questions` → Q1 (`gh-route-regex-builder`)
- Type: locate · Surface: github

> In `vercel/next.js` on `canary`, locate the exported `getRouteRegex()` function. Name its file, the internal helper it calls to parameterize the route, and the top-level fields returned by `getRouteRegex()`.

## G02

- Source: `compare/github-questions` → Q3 (`gh-flask-route-history`)
- Type: history · Surface: github

> In `pallets/flask`, identify the current file and owning base class for the `route` decorator. Then explain, from the changed code in commit `705e5268` rather than its title alone, what route-registration behavior it introduced.

## G03

- Source: `compare/github-questions` → Q5 (`gh-vue-pr-diff-review`)
- Type: history · Surface: github

> Review the code changes in `vuejs/core` PR `#15035`. Name at least two concrete hydration/interoperability scenarios fixed by the patch and explain why changes were required in both `runtime-core` and `runtime-vapor`.

## G04

- Source: `compare/github-questions` → Q6 (`gh-express-router-trace`)
- Type: trace · Surface: github

> On the current default branch of `expressjs/express`, determine whether the layer-matching loop lives in that repository. If not, cite the dependency that leads to the implementation repository, then name the function that advances layers and the helper that tests one layer against the path, with their files.

## G05

- Source: `compare/github-questions` → Q13 (`gh-redis-bitfield-security`)
- Type: history · Surface: github

> In `redis/redis`, identify the issue describing signed overflow in BITFIELD `#<offset>` parsing and the merged PR that fixes it. State the vulnerable operation and function, the approximate first overflowing `i64` offset, the files changed by the PR, and its additions/deletions.

## G06

- Source: `compare/github-questions` → Q17 (`gh-nextjs-fetch-memoization`)
- Type: semantic · Surface: github

> In `vercel/next.js`, explain how the App Router implements per-render fetch **request memoization** — i.e. how identical `fetch()` calls made during a single render are deduplicated. Identify: the function that installs the wrapped fetch onto the global `fetch`; the two layers it composes and the file each is defined in; the API used to scope the memoization to one render; what the deduplication cache key is derived from; and **all** conditions in the deduplication layer that bypass memoization and call the original fetch.

## G07

- Source: `compare/github-questions` → Q19 (`gh-node-child-process-dual-path`)
- Type: trace · Surface: github

> In `nodejs/node`, compare the implementation paths of `child_process.execFile()` and `child_process.execFileSync()`. Name the lower-level spawn function each uses, the implementation files and symbols that perform the process dispatch, and how a non-zero child exit is surfaced to the caller in each path.

## G08

- Source: `compare/github-questions` → Q22 (`gh-axios-tag-compare-range`)
- Type: history · Surface: github

> In `axios/axios`, use commit comparison between the release tags `v1.18.0` and `v1.19.0` (base `v1.18.0`, head `v1.19.0`). Report the ahead/behind commit counts for that range, then identify one commit in the range that changed a file under `lib/` and cite its SHA together with the diff hunk that proves the change — not the commit title. Explain how the range-comparison result differs from reading that single commit's diff on its own.

## G09

- Source: `compare/github-questions` → Q24 (`gh-axios-buildfullpath-blast-radius`)
- Type: multi-hop · Surface: github

> In `axios/axios`, locate the exported `buildFullPath` helper and name its file and current signature. Then enumerate every in-repo call site that consumes it (under `lib/`), citing `file:line` for each. For each call site, state which arguments it passes and whether a change to `buildFullPath`'s signature would be a breaking change or backward-compatible at that site. Distinguish call sites that would break from those that would not.

## G10

- Source: `compare/github-questions` → Q29 (`gh-mcp-http-auth-flow`)
- Type: semantic · Surface: github

> In `modelcontextprotocol/modelcontextprotocol`, explain how the Model Context Protocol handles **authorization** for the streamable HTTP transport. Identify the authorization framework it builds on (e.g. OAuth 2.x), the specification file that defines the flow, and describe the steps: how the client discovers the authorization server, how it obtains a token, and how that token is presented on subsequent HTTP requests. Cite the spec file and quote the lines that define the token-presentation header and the discovery mechanism.

## L01

- Source: `compare/terra-v3/suite/public-cases.json` → p01-langchain-runnable-config-declarations
- Type: locate · Surface: local
- Corpus: `langchain-ai/langchain@67ee6cb63dd9` at `octocode-local-testing/repos/langchain`

> At the locked LangChain commit, report every Python declaration of RunnableConfig under libs/core with exact file and line evidence.

## L02

- Source: `compare/terra-v3/suite/public-cases.json` → p08-next-react-create-context
- Type: locate · Surface: local
- Corpus: `vercel/next.js@d155ba9ebfff` at `octocode-local-testing/repos/nextjs`

> List direct generic createContext assignments under packages/next/src/client, returning the assigned symbol, type argument, file, and start position.

## L03

- Source: `compare/terra-v3/suite/public-cases.json` → p18-langchain-runnable-config-flow
- Type: trace · Surface: local
- Corpus: `langchain-ai/langchain@67ee6cb63dd9` at `octocode-local-testing/repos/langchain`

> Identify RunnableConfig's canonical declaration, Runnable.invoke and RunnableSequence.invoke consumers, and their immediate execution/config helpers, with exact evidence.

## L04

- Source: `compare/terra-v3/suite/public-cases.json` → p19-next-config-loading-flow
- Type: trace · Surface: local
- Corpus: `vercel/next.js@d155ba9ebfff` at `octocode-local-testing/repos/nextjs`

> Trace the locked Next.js configuration loader from exported loadConfig through loadConfigImpl to normalizeConfig and the frozen normalized value, with exact evidence.

## L05

- Source: `compare/terra-v3/suite/public-cases.json` → p20-cross-repo-tracing-extension-points
- Type: multi-hop · Surface: local
- Corpus: `langchain-ai/langchain@67ee6cb63dd9` at `octocode-local-testing/repos/langchain`; `vercel/next.js@d155ba9ebfff` at `octocode-local-testing/repos/nextjs`

> For LangChain and Next.js, identify one tracing extension point and its first internal dispatch step using exact source evidence.

## L06

- Source: `compare/advanced-research-v1/QUESTIONS.md` → A02 — Configuration cache collisions and observable work
- Type: multi-hop · Surface: local
- Corpus: `vercel/next.js@d155ba9ebfff` at `octocode-local-testing/repos/nextjs`

> In `/Users/bgaryy/code/octocode/octocode-local-testing/repos/nextjs`, analyze two sequential successful calls to the exported configuration loader in the same process, with identical project directory and phase. The first call uses a truthy customConfig object A and populates the cache. The second uses a different truthy customConfig object B, changes silent from true to false, supplies reportExperimentalFeatures, and otherwise uses identical options. Assume no cache mutation between calls. Does the second call evaluate B or reuse A's result? Enumerate the actual cache-key fields, explain how they form the key, and distinguish Boolean presence from object contents. Trace the hit through the implementation and exported wrapper: callback, rawConfig selection, error-state reset, default/adapter processing, React-version warning call, and timing log. Also explain whether freezing the normalized config proves deep immutability. Design a regression test with a fresh-load control that distinguishes the observed behavior from the claim that changing any loader option invalidates cache.

## L07

- Source: `compare/advanced-research-v1/QUESTIONS.md` → A10 — Route parameter encoding and optional catch-all semantics
- Type: multi-hop · Surface: local
- Corpus: `vercel/next.js@d155ba9ebfff` at `octocode-local-testing/repos/nextjs`

> In `/Users/bgaryy/code/octocode/octocode-local-testing/repos/nextjs`, trace route-regex generation, matching and interpolation for dynamic, catch-all and optional catch-all segments. Analyze absent segments and an encoded slash within one segment. Determine which layer decodes, which layer distinguishes arrays from scalars, and which error path handles malformed encoding. Cite existing tests and explain why finding a regex alone cannot establish the full behavior.

## L08

- Source: `compare/advanced-research-v1/QUESTIONS.md` → A03 — Retry only the failed batch members
- Type: semantic · Surface: local
- Corpus: `langchain-ai/langchain@67ee6cb63dd9` at `octocode-local-testing/repos/langchain`

> In `/Users/bgaryy/code/octocode/octocode-local-testing/repos/langchain`, trace RunnableRetry batch and abatch for three inputs where the middle input raises a configured retryable exception once and then succeeds, while the other inputs succeed immediately. Determine which members execute on the next attempt, how original result ordering is restored, how child callback tags identify attempts, and what happens at exhaustion with return_exceptions true versus false. Contrast batch retry with wrapping an entire RunnableSequence in retry. Cite the specific branches establishing the difference and one relevant existing test.

## L09

- Source: `compare/advanced-research-v1/QUESTIONS.md` → A04 — Fallback boundaries and exception injection
- Type: semantic · Surface: local
- Corpus: `langchain-ai/langchain@67ee6cb63dd9` at `octocode-local-testing/repos/langchain`

> In `/Users/bgaryy/code/octocode/octocode-local-testing/repos/langchain`, analyze RunnableWithFallbacks invoke, ainvoke, batch and abatch when the primary raises a handled exception and a fallback succeeds. Determine how exception_key changes the input contract, whether input dictionaries are mutated, which exception wins when all fallbacks fail, and whether an unhandled exception enters a fallback. Provide a behavior matrix supported by implementation and tests; do not assume the four entrypoints share identical control flow.

## L10

- Source: `compare/advanced-research-v1/QUESTIONS.md` → A06 — Streaming versus invoking a composed pipeline
- Type: semantic · Surface: local
- Corpus: `langchain-ai/langchain@67ee6cb63dd9` at `octocode-local-testing/repos/langchain`

> In `/Users/bgaryy/code/octocode/octocode-local-testing/repos/langchain`, compare stream/astream on a RunnableSequence containing a streaming producer, a RunnableLambda that cannot transform incrementally, and a streaming consumer. Locate the buffering boundary, explain when the first output becomes available, how chunks combine, and how callback error/end events are triggered. State the minimum implementation change needed to preserve incremental flow and identify a test that distinguishes streaming from a single buffered result.
