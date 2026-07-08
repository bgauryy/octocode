<code>
**Before writing** — stop at first yes: not needed? already exists? stdlib/platform? dep? one-line config?
  Each gate eliminates a whole class of wasted work: reimplementing what already exists creates divergence that compounds over time.
**Plan before editing** — check file locks (`workspace_status`); trace callers/consumers/contracts; define change and blast radius before touching anything.
  Blast radius = the full set of callers, type consumers, and runtime paths that break if this change is wrong. Know it before the first edit.

**Scope** — only changes directly requested or clearly necessary. Bug fixed = done; don’t add tests, refactor, or clean up unless asked.
**Bug fix** — find failure path first (failing test / trace / call site); mirror surrounding style, naming, and patterns.
**Contract** — trace real flow; find all callers/producers/consumers before changing. Modify the single owner; replace old paths instead of layering. Out-of-scope → cite `file:line`, do not fix.
  Layering instead of replacing splits responsibility between the old and new path — both must then be kept correct, which they won't be.

**Compatibility** — no shims unless required; remove legacy paths; no backward compat unless explicitly asked or public contract requires.

**Clean code** — names state intent not type · one function = one thing at one level (KISS) · guard-clause early returns · no magic numbers (name them) · no dead code or speculative params · comments explain why not what · boring over clever.
**Comments** — never attribute external sources, libraries, or prior art in code comments (e.g. no `// from CloakBrowser`, `// via puppeteer-extra`, `// source: X`); code must stand on its own.
**Shortcuts** — mark deliberate simplifications with a comment naming the ceiling and upgrade path (e.g. `// note: global lock; per-account if throughput matters`). Non-trivial logic → leave one runnable check (assert/small test); trivial one-liners need none.
  The ceiling comment is a debt marker: it signals to the next reader that the simplification was deliberate and bounded, not an oversight.

**Clean architecture** — concentric layers, dependencies point inward. Core (entities + use-cases) free of I/O / framework / transport / DB / UI; decouple via interfaces so they swap cheaply. Side effects at edges. Composition over inheritance; pure functions over shared mutable state. Abstract on the third use, not the first. Respect layer boundaries — never reach across, route through. Parse at boundaries; config via startup schema; deduplicate literals into constants. Document non-obvious rationale inline.

**FORBIDDEN** — stubs · placeholder wiring · looks-fixed patches · no-op boilerplate · inline suppressions · `_unused` naming · skipped/weakened tests · hardcoded green paths · suppressed lint/type errors. Implement the real path or state the blocker.
  Every item on this list shares the same failure mode: it moves a real problem from visible to hidden, making it harder to find and fix later.

**Errors** — no silent catches, fallbacks, or swallowing unless the contract requires it. Surface errors with context; fix the cause.
  Errors without context force the next debugger to reconstruct the original state; context at the throw site is the only chance to capture it cheaply.
**Retry** — if an approach fails, diagnose, adjust, retry once; never retry blindly.
</code>
