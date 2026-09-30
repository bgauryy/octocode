# Benchmark questions

Plain developer questions: the repository and ref (or PR/issue) and what the asker wants to know. No hints, suggested steps, file paths, or tool names. Local questions are answered against the read-only checkout at the pinned commit; the harness appends its path. Answer keys are judge-only, in `../references/`.

| ID | Category | Repository | Question |
|---|---|---|---|
| G01 | github-pr-review | fastapi/fastapi | Review fastapi/fastapi PR #16403 ("Add native OpenTelemetry support"). What does it change in runtime behavior, and what should a reviewer watch out for? |
| G02 | github-pr-review | pydantic/pydantic | Review pydantic/pydantic PR #13824 ("Add a `counter` core schema"). How does validation of `collections.Counter` fields behave after this PR compared to before? |
| G03 | github-pr-review | nodejs/undici | What bug does nodejs/undici PR #5881 ("fix(pool): ensure that removed clients don't pick up requests") fix, and how does the fix work? |
| G04 | github-pr-review | pallets/click | What does pallets/click PR #3866 deprecate, and which parameter declarations now produce a warning? |
| G05 | github-code-research | psf/requests | In psf/requests at commit 611c6162cb, how does a Session follow redirects, and what does it change about the request between hops? |
| G06 | github-code-research | encode/httpx | In encode/httpx at commit b5addb64f0, how does the synchronous Client decide whether a request goes through a proxy or connects directly? |
| G07 | github-code-research | Kludex/starlette | In Kludex/starlette at commit 63c5760d8a, how is an application's middleware stack assembled, and how do raised exceptions reach their handlers? |
| G08 | github-bug-rca | sveltejs/svelte | What is the root cause of sveltejs/svelte issue #18837, and how was it fixed? |
| G09 | github-bug-rca | pydantic/pydantic | What is the root cause of pydantic/pydantic issue #13786, and how was it fixed? |
| G10 | github-pr-review | tokio-rs/tokio | Review tokio-rs/tokio PR #8156 ("net: enable Miri tests for TCP socket"). What does it change, and which networking tests still don't run under Miri, and why? |
| L01 | local-trace | django/django | In django/django at commit 4fab678a07, how does the automatic redirect from a URL without a trailing slash to the one with a slash work? |
| L02 | local-locate | django/django | In django/django at commit 4fab678a07, where does Django stop you from saving an object whose foreign key points to an unsaved instance, and how does that check work? |
| L03 | local-trace | django/django | In django/django at commit 4fab678a07, how do callbacks registered with `transaction.on_commit` get stored, discarded and eventually run? |
| L04 | local-trace | langchain-ai/langchain | In langchain-ai/langchain at commit 67ee6cb63d, how does LangChain core's tool decorator build a tool's argument schema and description from a plain function when docstring parsing is enabled? |
| L05 | local-impact | langchain-ai/langchain | In langchain-ai/langchain at commit 67ee6cb63d, what in langchain-core would be affected if the public helper that merges streamed message-chunk content accepted exactly two contents instead of a variable number? |
| L06 | local-trace | vercel/next.js | In vercel/next.js at commit d155ba9ebf, how does the `generateEtags` config option end up affecting HTTP responses? |
| L07 | local-trace | vercel/next.js | In vercel/next.js at commit d155ba9ebf, how does calling the App Router's `redirect()` or `permanentRedirect()` on the server turn into an HTTP response? |
| L08 | local-trace | prometheus/prometheus | In prometheus/prometheus at commit ea954809ce, how is a scrape job's sample limit enforced? |
| L09 | local-semantic | prometheus/prometheus | In prometheus/prometheus at commit ea954809ce, how does Prometheus mark series as stale when they disappear from a scrape or when a target stops being scraped? |
| L10 | local-semantic | prometheus/prometheus | In prometheus/prometheus at commit ea954809ce, how does PromQL compute `rate()` and `increase()` over counter samples? |
| L11 | local-trace | tokio-rs/tokio | In tokio-rs/tokio at commit facc6fc47e, how does a spawned task's output get from the task to whoever awaits its `JoinHandle`? |
| L12 | local-semantic | tokio-rs/tokio | In tokio-rs/tokio at commit facc6fc47e, the multi-threaded scheduler sometimes runs a task woken by the currently running task next on the same worker. How does that optimization work, and what limits it? |
| L13 | local-trace | redis/redis | In redis/redis at commit 20bb2cfc54, what happens when a client runs GET on a string key whose TTL has already expired? |
| L14 | local-semantic | lodash/lodash | In lodash/lodash at commit 2b5e6f7399, how does `debounce` decide when to call the wrapped function, including what the `maxWait` option does? |
| L15 | local-semantic | google/guava | In google/guava at commit 4d41665af1, how do Guava's immutable hash sets defend against many elements with colliding hash codes? |
| L16 | local-trace | google/guava | In google/guava at commit 4d41665af1, how do `CacheBuilder`'s `maximumSize` and `concurrencyLevel` settings shape the cache's internal structure? |
| L17 | local-trace | JamesNK/Newtonsoft.Json | In JamesNK/Newtonsoft.Json at commit 52fa3aef1f, how are required members enforced when deserializing an object? |
| L18 | local-semantic | nlohmann/json | In nlohmann/json at commit f422b753cc, how does parsing with a parser callback that rejects some elements produce the resulting JSON value? |
| L19 | local-trace | microsoft/TypeScript | In microsoft/TypeScript at commit 4f5ddae224, how does the `moduleDetection` compiler option decide whether a file is treated as a module or a script in the Go-based compiler? |
| L20 | local-impact | microsoft/TypeScript | In microsoft/TypeScript at commit 4f5ddae224, what would be affected in the Go-based compiler if we changed how the default value of `resolveJsonModule` is computed? |
