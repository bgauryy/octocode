# GitHub Research v2 — Questions

Answer every requested claim and cite repository, ref, path, and a bounded
range or immutable GitHub object. If the available evidence does not support a
claim within the task budget, answer `Unknown` rather than infer it.

## Q1 — Route regex builder

In `vercel/next.js` on `canary`, locate the exported function that converts a
filesystem route string into a regular expression. Name its file and the
internal helper it calls first to create named parameter groups.

## Q2 — Repository discovery and bounded absence

Discover the GitHub repository owned by `sindresorhus` for the type-checking
utility package named `is`. Confirm its primary language and default branch,
then determine with bounded evidence whether its public export surface defines
or exports `isQuantumSuperposition`. Explain the search and evidence used for
the YES/NO answer.

## Q3 — Flask route history

In `pallets/flask`, identify the current file and owning base class for the
`route` decorator. Then explain, from the changed code in commit `705e5268`
rather than its title alone, what route-registration behavior it introduced.

## Q4 — Zustand fix PR state

In `pmndrs/zustand`, inspect fix PR `#3531`. Report its current state at
verification time, the source file it proposes to change, and the path-shape
edge case described by the PR.

## Q5 — Vue hydration diff review

Review the code changes in `vuejs/core` PR `#15035`. Name at least two concrete
hydration/interoperability scenarios fixed by the patch and explain why changes
were required in both `runtime-core` and `runtime-vapor`.

## Q6 — Express router cross-repository trace

On the current default branch of `expressjs/express`, determine whether the
layer-matching loop lives in that repository. If not, cite the dependency that
leads to the implementation repository, then name the function that advances
layers and the helper that tests one layer against the path, with their files.

## Q7 — Zustand's Next.js integration contract

Across `vercel/next.js` and `pmndrs/zustand`, determine whether
`examples/with-zustand/src/lib/store.ts` creates a module singleton or a React
Context-backed per-request store factory. Name the APIs used, then cite the
field in Zustand's root `package.json` that establishes whether React is a
required dependency or an optional peer.

## Q8 — VS Code keybinding dispatch

In `microsoft/vscode`, identify the concrete workbench keybinding service class
and file. Then identify the base class, file, and public method that receives a
keypress for dispatch.

## Q9 — Fastify lifecycle contract

In `fastify/fastify`, report the documented order from Incoming Request through
User Handler, including `onRequest`, `preParsing`, Parsing, `preValidation`,
Validation, and `preHandler`. Then identify the per-route context property and
runner function used by `lib/route.js` to invoke `onRequest` hooks.

## Q10 — Axios repository and Node entry chain

Discover the GitHub repository for Axios, report the dominant implementation
language from the repository language breakdown, and trace Node CommonJS
resolution from `main` through the relevant `exports` target to the underlying
source entry under `lib/`.

## Q11 — Esbuild repository and Node runtime boundary

Discover esbuild's source repository and report its dominant language from the
repository language breakdown. In that repository, determine whether the
normal Node API executes the core implementation in-process or communicates
with a separate process; name the Node APIs used.

## Q12 — Stream and EventEmitter wiring

In `nodejs/node`, identify where the base `Stream` constructor is implemented
and quote the prototype-wiring pattern that connects it to `EventEmitter`.
Then explain how `EventEmitter.prototype.once()` delegates listener
bookkeeping, naming the internal helpers and delegated methods.

## Q13 — Redis security issue and fix PR

In `redis/redis`, identify the issue describing signed overflow in BITFIELD
`#<offset>` parsing and the merged PR that fixes it. State the vulnerable
operation and function, the approximate first overflowing `i64` offset, the
files changed by the PR, and its additions/deletions.

## Q14 — Deep Agents evals PR review

Review `langchain-ai/deepagents` PR `#4338`. Report its state at verification
time, then from the changed files and PR description: (a) what the PR adds and
which dataset files are committed versus generated — and where the task data
comes from in CI; (b) the three evaluation arms and what each ladder step
isolates; (c) the loader's primary fetch path and its fallback; (d) the
scorer's provenance; (e) at least five changed paths, each classified as
adapter, dataset config, CI wiring, or agent graph.
