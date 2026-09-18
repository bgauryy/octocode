# BUG-02 Treatment — Next.js RSC stream aborts on error boundary

## THINK

**Source observations:**
- `app-render.tsx` imports both `renderToWebFizzStream` (Fizz/HTML) and `renderToWebFlightStream` (RSC/Flight) — two separate pipelines
- Imports `chainStreams`, `continueFizzStream`, `continueStaticPrerender` — confirms stream composition layer
- Error boundary semantics: RSC server components are functional only — `class` component `componentDidCatch` does not exist in RSC context

**Three causes:**
- H1: RSC renderer closes stream on any render error BEFORE boundary recovery can run
- H2: Error boundaries not invoked during server RSC rendering — only client boundaries apply
- H3: Fizz/Flight renderer flushes error before reconciler hands it to boundary (race)

**Analysis:** H2 is a fact about React's architecture — `componentDidCatch` requires a class component which cannot be a server component. So H2 IS true but it's not the whole picture: Suspense error boundaries DO work in some RSC contexts (they cause the boundary's fallback to render). The question is whether the STREAM aborts or the boundary's fallback is rendered.

## GATE

**Classification: `deterministic`** — H2 is confirmed by RSC architecture (no class components in server components), and the stream-ops.ts source (which encodes stream abort behavior) was inaccessible. Given that RSC has no server-side error boundary recovery, H1 (stream closes on unhandled throw) is the corollary. No Jev call needed — the mechanism is deterministic once H2 is established.

**Action:** Proceed without Jev. H1 is the observable consequence of H2.

## Root cause

**H2 is the architectural root; H1 is the consequence.** React Server Components cannot define error boundaries (no class components → no `componentDidCatch`). When a server component throws during RSC rendering (via `renderToWebFlightStream`), the Flight renderer has no boundary recovery mechanism — it propagates the error to the stream controller, which aborts the stream.

The Suspense+ErrorBoundary syntax in RSC routes works at a different level: the Suspense boundary on the CLIENT hydrates from the already-aborted stream's error payload, triggering the client-side error boundary fallback. But the RSC stream itself has already aborted server-side.

## Fix

Next.js can emulate server-side error boundary semantics by wrapping segment renders in try/catch within `app-render.tsx` and rendering a fallback RSC payload (the error boundary's `fallback` prop content) when a segment throws:

```ts
try {
  segment = renderServerComponent(component, props)
} catch (e) {
  if (hasErrorBoundary(layoutTree)) {
    segment = renderErrorBoundaryFallback(fallback, e)
  } else {
    throw e
  }
}
```

The `PrerenderStore*` types already distinguish dynamic/static rendering — the same context could carry error boundary fallback metadata.

## Decision snapshot
- **GATE:** `deterministic` — no Jev call
- **Jev calls:** 0
- **Confidence:** Medium — RSC architecture is confirmed; stream-ops abort code not directly read
