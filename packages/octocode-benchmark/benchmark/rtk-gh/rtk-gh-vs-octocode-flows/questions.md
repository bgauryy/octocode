# GitHub Research-Flow Questions (solver-facing) — v2 (10 questions)

v2 (2026-07-12): replaced v1's Q3 with a deep multi-file PR review and added
Q7–Q10. v1 (6Q) results are not comparable — see `../README.md`.

10 remote-GitHub questions across `pmndrs/zustand`, `vercel/next.js`,
`microsoft/vscode`, `vuejs/core`, `sveltejs/svelte`, `nodejs/node`,
`evanw/esbuild`, `fastify/fastify`, `redis/redis` (no `facebook/react`; spans
TS, C, Go). Each exercises a different flow — don't assume one tool sequence
solves all ten. Q6 is open-ended/exploratory, no single pinpoint fact expected.

Answer every question with: the answer, evidence anchors (`file:line`, PR/issue
number, or sha), confidence (high/medium/low), and steps used.

| Q | Flow category | Repo(s) |
|---|---|---|
| Q1 | Cross-repo comparison | `pmndrs/zustand` + `vercel/next.js` |
| Q2 | How-it-works / flow trace | `vercel/next.js` |
| Q3 | Deep/large PR review | `vuejs/core` |
| Q4 | Bug/issue validation (root-cause style) | `pmndrs/zustand` |
| Q5 | Find-in-large-repo | `microsoft/vscode` |
| Q6 | Exploratory cross-repo comparison (open-ended) | `vuejs/core` + `sveltejs/svelte` |
| Q7 | Deep dive / architecture exploration | `nodejs/node` |
| Q8 | npm package → source-repo research | `esbuild` npm pkg → `evanw/esbuild` |
| Q9 | How-it-works / flow trace #2 | `fastify/fastify` |
| Q10 | Root-cause analysis #2 (security bug) | `redis/redis` |

## Q1 — cross-repo comparison

Both `pmndrs/zustand` and `vercel/next.js` ship an official integration example
(`vercel/next.js` repo path `examples/with-zustand/`). (a) In that example, does
`src/lib/store.ts` create the store through a plain module-level singleton, or
through a React `Context` (`createContext`/`useContext`) factory — give the file
path and name the exact API it wraps? (b) In zustand's own `package.json`
(repo root), is `react` listed as a REQUIRED dependency or an OPTIONAL peer
dependency (cite the exact field)? (c) In one sentence, connect (a) and (b): why
would a per-request Context factory matter for a library that treats React as
optional?

## Q2 — how something works (Next.js internals)

Next.js converts a file-system route pattern such as `app/blog/[slug]/page.tsx`
into a matcher usable at request time. (a) Which exported function performs the
string→regex conversion (name, file, line)? (b) Which internal helper function
does it call FIRST to tokenize the route into named parameter groups (name,
file, line)? Both live in `vercel/next.js`.

## Q3 — deep/large PR review

Find and review `vuejs/core` PR **#15035** ("fix(runtime-vapor): preserve
VNode anchors in dynamic component hydration"). This is a real multi-file
runtime bugfix, not a docs change — review it like you would before approving
it. Report: (a) the total files changed and net line delta (additions/
deletions); (b) which files are actual SOURCE (non-test) changes, across which
TWO packages under `packages/`; (c) of those source files, which single one
has the largest combined (additions+deletions) diff, and roughly how large; (d)
in your own words, what class of hydration bug is this PR fixing — name at
least two of the specific interop scenarios (e.g. Teleport, async components,
slot fallback) it addresses, and why anchoring/cleanup logic for them lives
partly in `packages/runtime-core` and partly in `packages/runtime-vapor`.

## Q4 — bug / issue validation

`pmndrs/zustand` discussion **#3530** reports that the devtools middleware's V8
stack-trace regex mis-captures the caller name when the source path contains a
space. Do NOT trust any PR's claim at face value — verify against the CURRENT
`main` branch of `src/middleware/devtools.ts`: (a) what is the exact regex
literal currently assigned to `v8StackLineRe` (cite file+line)? (b) Is the fix
PR for this bug merged or still open (give the PR number and its state)? (c)
State plainly whether the bug is still live in `main` right now.

## Q5 — find in a large repo

`microsoft/vscode` is a large multi-package monorepo. (a) Which concrete class
is the one actually wired into the workbench as the runtime keybinding service
(name, file)? (b) The real keypress→command dispatch entry point is defined on
a DIFFERENT (base) class than the one in (a) — name that base class, its file,
and the dispatch method's name + line number.

## Q6 — exploratory cross-repo comparison (open-ended)

Compare the core rendering/update mechanism of Vue 3 (`vuejs/core`) and Svelte
(`sveltejs/svelte`) — this is exploratory, go as deep as useful within your
step budget, there is no single required anchor:

(a) In `vuejs/core`, does the runtime maintain a virtual DOM that gets
diffed/patched whenever reactive state changes? Name the core function that
performs this diff/patch step and its file.

(b) In `sveltejs/svelte`, does the COMPILED component runtime perform an
equivalent virtual-DOM-diffing step at all, or does it call pre-determined,
granular DOM-manipulation functions directly? Name at least two such
functions and the file they live in.

(c) In 2–4 sentences, explain the architectural trade-off this reveals
(compile-time work and emitted-code size vs. runtime diffing cost and bundle
size). Bonus (not required): is Svelte's approach ever NOT fully
diff-free at runtime — is there any file whose whole job is runtime
reconciliation of a dynamic collection?

## Q7 — deep dive / architecture exploration (nodejs/node)

`nodejs/node`'s public `lib/stream.js` is a thin ~150-line aggregator, not the
real implementation. Dig into `lib/internal/streams/` and `lib/events.js` to
answer: (a) Where does the base `Stream` constructor actually live, and how
is its prototype chain wired to `EventEmitter` — quote the specific pattern
used (this codebase predates/avoids `class X extends Y` in this exact spot;
say what it uses instead) and name the file. (b) Which single file under
`lib/internal/streams/` is by far the largest (holds the real `Readable`
base-class implementation) — name it, and name the next-largest file in that
same directory for comparison. (c) In `lib/events.js`, does
`EventEmitter.prototype.once()` reimplement listener bookkeeping itself, or
does it just build a wrapper and delegate to `.on()`/`.removeListener()` —
name the internal helper function(s) involved.

## Q8 — npm package → source-repo research

Start from the npm package `esbuild` (~250M+ weekly downloads) using
`npmSearch` (or the whitelisted equivalent). (a) Which GitHub org/repo is its
source? (b) The published npm package's `lib/` folder is small and looks like
plain JS/TS — but by GitHub's own per-language byte breakdown for that repo,
what is the actual DOMINANT implementation language of the codebase (not the
published npm package's `lib/` folder — the full source repo)? (c) Trace how
the Node.js-side JS API actually runs the real implementation at request time:
does it compile the core logic into JS/WASM and run it in-process, or does it
spawn a separate native process and talk to it? If the latter, name the
concrete Node core module/API used to launch and communicate with it.

## Q9 — how something works (Fastify request lifecycle)

Per Fastify's own lifecycle documentation, trace a request from arrival to
response: (a) List the ordered phases from "Incoming Request" through the
User Handler running, explicitly placing `onRequest`, `preParsing`,
`preValidation`, and `preHandler` in their correct relative order (and where
the `Parsing` and `Validation` steps sit relative to those hooks). (b) After
the User Handler produces a reply, name the TWO hooks that run, in order,
before the response is actually written to the socket. (c) In the fastify
source (`lib/route.js` / `lib/hooks.js`), which per-route-context property
does the request-handling code check BEFORE it runs the `onRequest` hooks for
a matched route, and what is the name of the function that actually runs them?

## Q10 — root-cause analysis (redis/redis, security bug)

`redis/redis` had a real reported denial-of-service bug in `BITFIELD` /
`BITFIELD_RO`'s `#<offset>` parsing. (a) Find the GitHub issue reporting it —
give its number and, in your own words, the exact root-cause mechanism (name
the unsafe operation and the function it occurs in, and roughly how large an
offset triggers it for an `i64` field). (b) Find the merged PR that fixed it —
give its number, the file(s) it touched, and the net line delta. (c) Describe
the fix: what check was added, and where relative to the unsafe operation? (d)
Do NOT trust the PR description at face value — verify against the CURRENT
`redis/unstable` branch: is the guard actually present in `src/bitops.c` right
now? Quote the specific condition and roughly where (line/function) it sits.
