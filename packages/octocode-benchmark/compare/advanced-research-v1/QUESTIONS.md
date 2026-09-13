# Advanced repository research questions

Public diagnostic suite, 2026-09-12. These are new questions, not amendments to the
frozen Terra v3 public or private suites. Only A01 and A02 initially have reviewed
source-grounded scoring rubrics; A03–A12 are candidates pending oracle review.

Corpus: LangChain `67ee6cb63dd9ae7f3a4dfedc3095652bce15a125` and
Next.js `d155ba9ebfffe4742efefda8d68c2e0e8e490924`. `$LANGCHAIN` and `$NEXTJS`
are template labels resolved to literal clone paths before delivery. Answer from these bytes, cite exact
repository-relative file:line evidence, distinguish observation from inference,
and state any incomplete coverage. Do not execute application code or change files.

## A01 — Configuration precedence and child-run identity

In `$LANGCHAIN`, consider `merge_configs(A, B)` with no inherited context config.
A has tags `['z', 'a']`, metadata `{'owner': 'A', 'nested': {'a': 1},
'lc_versions': {'core': '1', 'partner': '1'}}`, configurable
`{'model': 'mA', 'temperature': 0.1}`, recursion_limit 7, and run_name `'parent'`.
B has tags `['b', 'a']`, metadata `{'owner': 'B', 'nested': {'b': 2},
'lc_versions': {'core': '2'}}`, configurable `{'model': 'mB', 'temperature': 0.2,
'checkpoint_ns': 'ns'}`, and recursion_limit equal to DEFAULT_RECURSION_LIMIT.
Compute the resulting tags, metadata, configurable values, recursion_limit and
run_name, including automatically introduced metadata. Explain whether an arbitrary
configurable scalar is copied into metadata and whether nested metadata merges
recursively. Then trace how RunnableSequence.invoke prepares a child's config:
what happens to run_name and run_id when callbacks are replaced, which step receives
invocation kwargs, and which parent callbacks run when every step succeeds versus
when a step raises? Distinguish
top-level copying from a promise of deep immutability.

## A02 — Configuration cache collisions and observable work

In `$NEXTJS`, analyze two sequential successful calls to the exported configuration
loader in the same process, with identical project directory and phase. The first
call uses a truthy customConfig object A and populates the cache. The second uses a
different truthy customConfig object B, changes silent from true to false, supplies
reportExperimentalFeatures, and otherwise uses identical options. Assume no cache
mutation between calls. Does the second call evaluate B or reuse A's result?
Enumerate the actual cache-key fields, explain how they form the key, and distinguish
Boolean presence from object contents. Trace the hit through the implementation and exported wrapper: callback,
rawConfig selection, error-state reset, default/adapter processing, React-version
warning call, and timing log. Also explain whether freezing the normalized config
proves deep immutability. Design a regression test with a fresh-load control that
distinguishes the observed behavior from the claim that changing any loader option
invalidates cache.

## A03 — Retry only the failed batch members

In `$LANGCHAIN`, trace RunnableRetry batch and abatch for three inputs where the
middle input raises a configured retryable exception once and then succeeds, while
the other inputs succeed immediately. Determine which members execute on the next
attempt, how original result ordering is restored, how child callback tags identify
attempts, and what happens at exhaustion with return_exceptions true versus false.
Contrast batch retry with wrapping an entire RunnableSequence in retry. Cite the
specific branches establishing the difference and one relevant existing test.

## A04 — Fallback boundaries and exception injection

In `$LANGCHAIN`, analyze RunnableWithFallbacks invoke, ainvoke, batch and abatch when
the primary raises a handled exception and a fallback succeeds. Determine how
exception_key changes the input contract, whether input dictionaries are mutated,
which exception wins when all fallbacks fail, and whether an unhandled exception
enters a fallback. Provide a behavior matrix supported by implementation and tests;
do not assume the four entrypoints share identical control flow.

## A05 — Cancellation, executor context, and exception translation

In `$LANGCHAIN`, trace the default Runnable.ainvoke path into synchronous invoke and
the executor/context helper. Explain what context reaches the worker, how
StopIteration is handled, and what source evidence does or does not establish about
stopping the worker when the awaiting task is cancelled. Compare a native async
override and identify a test for context propagation. Do not equate cancellation of
an awaiter with termination of a thread.

## A06 — Streaming versus invoking a composed pipeline

In `$LANGCHAIN`, compare stream/astream on a RunnableSequence containing a streaming
producer, a RunnableLambda that cannot transform incrementally, and a streaming
consumer. Locate the buffering boundary, explain when the first output becomes
available, how chunks combine, and how callback error/end events are triggered.
State the minimum implementation change needed to preserve incremental flow and
identify a test that distinguishes streaming from a single buffered result.

## A07 — Callback inheritance and duplicate handlers

In `$LANGCHAIN`, derive the behavior of merge_configs when callbacks are each of
None, a handler list, and a callback manager. Trace manager copying, manager merging,
inheritance flags, and handler identity through the manager implementation. Determine
whether the same handler object can receive duplicate events for each combination;
separate a lexical appearance of a handler from a proven dispatch path.

## A08 — Configuration loading paths and normalization order

In `$NEXTJS`, compare standalone configuration, customConfig, and configuration
loaded from a file. Trace async/function normalization, validation, defaults,
adapter.modifyConfig, cache population, and rawConfig behavior in each branch.
Provide a branch matrix and a counterexample to an overly broad claim that every
successful configuration passes through every stage. Include exact evidence across
the loader, normalizer and adapter contract.

## A09 — Revalidation across request boundaries

In `$NEXTJS`, follow a cache-tag invalidation from its public server API to the
pending work recorded on the current request and the cache-read paths that consult
revalidated tags. Distinguish a tag invalidated earlier in a request from one received
from a previous request, and distinguish stale-while-revalidate from immediate
expiration where supported at this commit. Give one concrete timeline and identify
the branches that prevent reuse of an invalid entry. State coverage limits.

## A10 — Route parameter encoding and optional catch-all semantics

In `$NEXTJS`, trace route-regex generation, matching and interpolation for dynamic,
catch-all and optional catch-all segments. Analyze absent segments and an encoded
slash within one segment. Determine which layer decodes, which layer distinguishes
arrays from scalars, and which error path handles malformed encoding. Cite existing
tests and explain why finding a regex alone cannot establish the full behavior.

## A11 — Scope-complete structural enumeration

In `$NEXTJS/packages/next/src/client`, enumerate direct variable initializers that
call createContext with a type argument in both TypeScript and TSX, including qualified
React.createContext calls if present. Exclude strings, comments, nested calls used as
another initializer's argument, and unrelated same-named declarations. Report complete
file/start-position/name/type-expression tuples, explicit empty categories, and any
symbol-identity uncertainty. Demonstrate that pagination and language filters did not
silently omit matches. Freeze an independently reproduced exact-set oracle before scoring.

## A12 — Cross-repository tracing lifecycle

Compare `$LANGCHAIN` callback tracing and `$NEXTJS` client tracing. For one successful
and one failing operation in each, trace creation, parent linkage, terminal event and
subscriber dispatch. Identify extension points and cleanup guarantees versus inferred
behavior. Give a change-impact set for adding a terminal cancellation category, separating
direct consumers, tests, and dynamic/unsupported edges. A syntactic graph or a zero-reference
result alone must not establish that code is safe to delete.
