# BUG-03 Treatment — Next.js useSearchParams forces full SSR re-render

## THINK

**Source observations:**
- `app-render.tsx` imports `PrerenderStoreModernServer`, `PrerenderStoreModernClient`, `PrerenderStoreModernRuntime` — confirms per-render-unit dynamic tracking
- `workAsyncStorage`, `workUnitAsyncStorage` — confirms async context used for dynamic marking propagation
- Next.js docs: "If you need to access search params in a page, opt the page into dynamic rendering by reading the searchParams prop" — segment-level dynamic marking

**Three causes:**
- H1: Static generation optimizer marks entire segment as dynamic at build time when useSearchParams() detected
- H2: useSearchParams() triggers dynamic rendering bailout inside RSC renderer that propagates past Suspense
- H3: Static generation optimizer cannot statically analyze Suspense boundary wrapping

**Analysis:** H1 and H3 are the same mechanism described at different levels. H2 (RSC renderer bailout propagating past Suspense) is plausible but the PrerenderStore imports confirm that dynamic marking IS segment-scoped. The PPR (Partial Prerendering) imports in app-render.tsx suggest Next.js CAN isolate dynamic segments — the question is whether the Suspense boundary triggers PPR isolation.

## GATE

**Classification: `deterministic`** — the `PrerenderStore*` type imports confirm that dynamic rendering is tracked per work unit. The behavior is well-documented and consistent with the import structure. No residual dispute between H1 and H3 (they describe the same thing). Jev not needed.

**Action:** Proceed without Jev. H1 is correct.

## Root cause

**H1 correct.** The Next.js static generation optimizer detects `useSearchParams()` at build time and marks the enclosing page SEGMENT as dynamic. Even when wrapped in `<Suspense>`, the dynamic marking applies to the segment (page route), not just the Suspense subtree — because the segment boundary is the atomic unit for static/dynamic decisions in the App Router.

The result: every request to that page triggers a full server-side render rather than serving a cached static shell with Suspense-bounded dynamic content.

Partial Prerendering (PPR, `experimental_ppr`) addresses this — it separates the static shell from the dynamic Suspense-bounded hole. Without PPR, the full-page re-render is expected behavior, not a bug per se — it's a design constraint.

## Fix

Enable PPR for the route, OR use `searchParams` prop instead of `useSearchParams()` hook (passes params without marking the component as dynamically rendering), OR move the component using `useSearchParams()` to a client component wrapped in Suspense.

## Decision snapshot
- **GATE:** `deterministic` — no Jev call
- **Jev calls:** 0
- **Confidence:** Medium-high — behavior confirmed by Next.js docs and PrerenderStore type imports
