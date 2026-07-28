# Octocode vs (rtk + gh) — Questions

## Q1

Both `pmndrs/zustand` and `vercel/next.js` ship an integration example
(`vercel/next.js` path `examples/with-zustand/`). (a) In that example, does
`src/lib/store.ts` create the store as a module-level singleton or through a
React `Context` factory — give the file path and the exact API it wraps. (b) In
zustand's `package.json` (repo root), is `react` a required dependency or an
optional peer dependency — cite the field. (c) Why would a per-request Context
factory matter for a library that treats React as optional?

## Q2

Next.js converts a filesystem route pattern such as `app/blog/[slug]/page.tsx`
into a request-time matcher. (a) Which exported function performs the
string→regex conversion (name, file, line)? (b) Which internal helper does it
call first to tokenize the route into named parameter groups (name, file, line)?

## Q3

`vuejs/core` PR **#15035** ("fix(runtime-vapor): preserve VNode anchors in
dynamic component hydration"). (a) Total files changed and net line delta. (b)
Which files are SOURCE (non-test) changes, across which two packages under
`packages/`. (c) Of those source files, which single one has the largest combined
(additions+deletions) diff, and roughly how large. (d) What class of hydration
bug is this PR fixing — name at least two specific interop scenarios it
addresses, and why the fix spans both `packages/runtime-core` and
`packages/runtime-vapor`.

## Q4

`pmndrs/zustand` discussion **#3530** reports the devtools middleware's V8
stack-trace regex mis-captures the caller name when the source path contains a
space. In the current `main` branch of `src/middleware/devtools.ts`: (a) the
exact regex literal assigned to `v8StackLineRe` (file+line). (b) The fix PR's
number and state (merged or open). (c) Is the bug live in `main` now?

## Q5

`microsoft/vscode`. (a) Which concrete class is wired into the workbench as the
runtime keybinding service (name, file)? (b) The keypress→command dispatch entry
point is defined on a different base class — name that base class, its file, and
the dispatch method's name + line number.

## Q6

Compare the core rendering/update mechanism of Vue 3 (`vuejs/core`) and Svelte
(`sveltejs/svelte`). (a) In `vuejs/core`, does the runtime maintain a virtual DOM
that gets diffed/patched on reactive state change? Name the core diff/patch
function and its file. (b) In `sveltejs/svelte`, does the compiled component
runtime perform an equivalent virtual-DOM diff, or call granular DOM-manipulation
functions directly? Name at least two such functions and their file. (c) The
architectural trade-off this reveals (compile-time work / emitted-code size vs.
runtime diffing cost / bundle size). (d) Is any Svelte file's whole job runtime
reconciliation of a dynamic collection?

## Q7

`nodejs/node`. (a) Where does the base `Stream` constructor live, and how is its
prototype chain wired to `EventEmitter` — quote the specific pattern used
(not `class X extends Y`) and name the file. (b) Which single file under
`lib/internal/streams/` is by far the largest, and which is the next-largest? (c)
In `lib/events.js`, does `EventEmitter.prototype.once()` reimplement listener
bookkeeping or wrap and delegate to `.on()`/`.removeListener()` — name the
internal helper function(s).

## Q8

The npm package `esbuild`. (a) Its GitHub source org/repo. (b) By the repo's
per-language byte breakdown, the dominant implementation language of the full
source repo. (c) At request time, does the Node JS API compile the core logic
into JS/WASM and run it in-process, or spawn a separate native process and talk
to it? If the latter, name the Node core module/API used.

## Q9

`fastify/fastify`. (a) The ordered request lifecycle phases from "Incoming
Request" through the User Handler, placing `onRequest`, `preParsing`,
`preValidation`, `preHandler`, and the `Parsing`/`Validation` steps in order. (b)
After the User Handler produces a reply, the two hooks that run, in order, before
the response is written. (c) In `lib/route.js`/`lib/hooks.js`, which per-route
context property is checked before the `onRequest` hooks run for a matched route,
and what function runs them?

## Q10

`redis/redis` had a denial-of-service bug in `BITFIELD` / `BITFIELD_RO`
`#<offset>` parsing. (a) The GitHub issue number and the root-cause mechanism
(the unsafe operation, the function it occurs in, and roughly how large an offset
triggers it for an `i64` field). (b) The merged fix PR's number, the file(s) it
touched, and the net line delta. (c) The fix: what check was added, and where
relative to the unsafe operation. (d) In the current `redis/unstable`
`src/bitops.c`, is the guard present? Quote the condition and roughly where.
