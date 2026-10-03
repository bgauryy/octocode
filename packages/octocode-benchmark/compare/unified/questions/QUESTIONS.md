# Benchmark questions

Plain developer questions: the repository and ref (or PR/issue) and what the asker wants to know. No hints, suggested steps, file paths, or tool names. Local questions are answered against the read-only checkout at the pinned commit; the harness appends its path. Answer keys are judge-only, in `../references/`.

Every question pins a commit (repo@sha) or a release (package@version, resolved to its tag commit in `questions.json`). IDs starting with `G` need only GitHub; IDs starting with `L` include a local checkout. The `L` prefix also covers the `mixed` questions, which need the local checkout and GitHub or a package registry together.

| Category | n | What it asks |
|---|--:|---|
| github-pr-review, github-code-research, github-bug-rca | 10 | A PR, an issue's root cause, or code behavior in a GitHub repository |
| github-structure | 2 | How a repository at a ref is laid out |
| github-repo-discovery | 2 | Which repository is the right one, then one fact at a release |
| github-history | 2 | Which PRs, issues or commits made a change, when the numbers are not given |
| artifact | 2 | Registry facts of a package release, plus its release source |
| local-trace, local-locate, local-semantic, local-impact | 20 | Behavior, location or change impact in a local checkout |
| local-symbol | 2 | Same-name symbols or callers, where a text search is ambiguous |
| local-structure | 2 | Structural patterns or outlines, such as the enclosing function of each match |
| local-described-target | 2 | A behavior described in words, inside a file of more than 1,000 lines |
| mixed | 5 | Local code with its upstream dependency, a GitHub issue or PR, or a published release |

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
| G11 | github-structure | encode/httpx | In encode/httpx at commit b5addb64f0, which transport implementations ship with the package, which module defines each one, and which of them can be used with the synchronous `Client`, the `AsyncClient`, or both? |
| G12 | github-structure | fastapi/fastapi | In fastapi/fastapi at commit 4b3949cd9e, how is the `fastapi.security` package organized: which modules does it contain, which public classes does each module define, and which common base class do the security schemes share? |
| G13 | github-repo-discovery | prometheus/node_exporter | Which GitHub repository hosts the Prometheus project's official exporter for machine-level (host) metrics, and in its v1.8.2 release, what listen address, metrics path and maximum number of parallel scrape requests does it use by default? |
| G14 | github-repo-discovery | prometheus/client_golang | Which GitHub repository hosts the Prometheus project's official Go instrumentation library, and in its v1.20.0 release, which bucket boundaries does a histogram get when none are configured, and when are those defaults not applied? |
| G15 | github-history | tokio-rs/tokio | In tokio-rs/tokio, as of commit facc6fc47e the runtime has an unstable, opt-in sharded queue for `spawn_blocking` tasks. Which pull request added that opt-in, and what happened to the earlier attempt to shard that queue? |
| G16 | github-history | prometheus/prometheus | In prometheus/prometheus, as of commit ea954809ce, which pull request added support for scraping targets over Unix domain sockets, which issue did it close, and which later pull request fixed a connection mix-up in that feature, and what was the mix-up? |
| G17 | artifact | psf/requests | For the PyPI release requests==2.32.3: which Python versions and runtime dependencies (including optional extras) does it declare, where is its source for that release, and what connection-pool and retry defaults does its HTTP adapter use? |
| G18 | artifact | nodejs/undici | For the npm package undici@6.21.0: which Node.js versions does it declare support for, which source commit was it published from, and in that release what default timeouts does a `Client` use for response headers, response bodies and keep-alive connections? |
| L21 | local-symbol | tokio-rs/tokio | In tokio-rs/tokio at commit facc6fc47e, the blocking thread pool has several functions named `begin_shutdown`. When the pool shuts down, which of them run and in what order, and what does each one do beyond the one it calls? |
| L22 | local-symbol | django/django | In django/django at commit 4fab678a07, several functions are named `static`. Which code in the `django` package itself (not the tests) calls the one that turns a relative static-asset path into a URL, as opposed to the URL-pattern helper or the template context processor? |
| L23 | local-structure | django/django | In django/django at commit 4fab678a07, which methods of `QuerySet` clone the queryset by calling `self._chain()` directly? |
| L24 | local-structure | prometheus/prometheus | In prometheus/prometheus at commit ea954809ce, which functions in the scrape package (non-test code) start goroutines, and what does each of those goroutines do? |
| L25 | local-described-target | lodash/lodash | In lodash/lodash at commit 2b5e6f7399, how does deep equality comparison (as used by `_.isEqual`) avoid infinite recursion when the compared values contain references back to themselves, and when does it consider two such self-referencing values equal? |
| L26 | local-described-target | django/django | In django/django at commit 4fab678a07, when you call `save()` on a model instance without forcing an insert or an update, how does Django decide whether to run an UPDATE or an INSERT for the instance's table, and what happens when the UPDATE matches no row? |
| L27 | mixed | prometheus/prometheus, prometheus/common | In prometheus/prometheus at commit ea954809ce, a scrape config's `metric_name_validation_scheme` setting is parsed into a type that comes from a dependency. Which module and version provide that type, according to the checkout's dependency pins, and at that version, which values does it accept and what does each value allow in metric and label names? |
| L28 | mixed | redis/redis | redis/redis issue #15874 reports that ARLASTITEMS can leave existing elements out of its reply. In redis/redis at commit 20bb2cfc54, where is the code that causes this, why does it drop elements, and how did the fix change the command's behavior? |
| L29 | mixed | tokio-rs/tokio | tokio-rs/tokio issue #8541 reports a use-after-free in the semaphore when a tracing subscriber panics. In tokio-rs/tokio at commit facc6fc47e, where is the tracing event that makes this possible, and why does a panic at that point leave a completed waiter linked in the wait queue? |
| L30 | mixed | tokio-rs/tokio | tokio-rs/tokio PR #8546 fixes an index-wraparound bug in the mpsc channel. Does tokio-rs/tokio at commit facc6fc47e still have that bug? Where are the affected spots in that checkout, and what does the PR change at each one? |
| L31 | mixed | JamesNK/Newtonsoft.Json | The NuGet package Newtonsoft.Json 13.0.3 ships builds for a set of target frameworks. Compared with that release, which target frameworks does JamesNK/Newtonsoft.Json at commit 52fa3aef1f build: which were added and which were dropped? |
