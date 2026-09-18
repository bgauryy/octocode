# BUG-02 Baseline — Next.js RSC stream aborts on error boundary catch

## Root cause

**H1 and H3 both contribute, with H1 being primary for the App Router RSC path.** From `packages/next/src/server/app-render/app-render.tsx`, the imports include `continueFizzStream`, `renderToWebFizzStream`, `renderToWebFlightStream` — confirming the streaming pipeline uses separate Flight (RSC) and Fizz (HTML) streams.

For the RSC (Flight) stream: when rendering server components, `renderToWebFlightStream` produces the RSC payload. In the React Flight renderer, if a component throws during rendering and React's reconciler hasn't yet identified a nearest error boundary (because error boundaries are a React client concept and the Flight renderer is fundamentally different), the unhandled throw causes the Flight writer to abort the stream.

The key distinction: React **server components cannot have error boundaries** — error boundaries are `class` components with `componentDidCatch`, and server components are always functional. So the RSC stream has no recovery mechanism: any throw that isn't caught by a `try/catch` in the server component rendering path propagates to the stream controller and aborts it.

The Suspense+ErrorBoundary pattern works on the CLIENT because the client hydrates the HTML skeleton and the client-side React reconciler handles the error boundary. But the RSC stream itself (the `.rsc` payload) aborts server-side before the client ever sees it.

## Fix proposal

The fix requires wrapping server component renders in a try/catch at the segment level in `app-render.tsx` and rendering a fallback/error RSC payload when a segment throws, rather than aborting the stream. The error boundary semantics need to be emulated server-side:

```ts
// In the server component rendering loop
try {
  renderSegment(...)
} catch (e) {
  if (isErrorBoundaryError(e)) {
    renderErrorBoundaryFallback(e)
  } else {
    throw e  // propagate fatal errors
  }
}
```

## Evidence

- `packages/next/src/server/app-render/app-render.tsx`: stream pipeline imports confirmed — `renderToWebFlightStream`, `renderToWebFizzStream`, `continueFizzStream`
- React server component architecture: no class components → no `componentDidCatch` → no server-side error boundaries

## Confidence

**Medium** — The streaming abort mechanism is well-understood from RSC architecture. The exact stream abort callsite was not directly read (stream-ops.ts inaccessible).
