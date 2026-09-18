# BUG-03 Baseline — Next.js useSearchParams forces full SSR re-render

## Root cause

**H1 is correct.** The Next.js static generation optimizer (the "dynamic rendering" analyzer) walks the component tree at build time to determine which routes can be statically generated. When `useSearchParams()` is detected in a component, the optimizer marks the SEGMENT — not just the component — as dynamic, because search params are inherently dynamic (they're not known at build time).

This marking happens at the route segment level in `packages/next/src/server/app-render/` during the static generation pass. Even when `useSearchParams()` is wrapped in a `<Suspense>` boundary, the static analysis marks the entire enclosing segment as requiring dynamic rendering, because the analyzer cannot guarantee that the Suspense boundary will fully isolate the dynamic dependency from the segment's static shell.

The Suspense boundary DOES prevent the dynamic rendering from blocking the outer layout (the layout can still be statically generated), but the PAGE SEGMENT containing `useSearchParams()` becomes fully dynamic — hence "full SSR re-render" of that page component on every request rather than serving a static HTML shell with client-side hydration of the search-params-dependent subtree.

## Fix proposal

Next.js should thread the Suspense boundary's presence through the static analyzer so that when `useSearchParams()` is detected inside a Suspense boundary, the dynamic marking is applied only to the Suspense subtree and not the enclosing segment. The `app-render` pipeline already supports "partial static rendering" via `ppr` — this same mechanism could apply here:

```
If useSearchParams() is inside <Suspense>:
  → Mark only the Suspense subtree as dynamic
  → Allow the enclosing segment to remain statically renderable
```

## Evidence

- `packages/next/src/server/app-render/app-render.tsx`: imports `PrerenderStoreModernServer`, `PrerenderStoreModernClient` — confirms static/dynamic split is tracked per render unit
- Next.js docs: "If you need to access search params in a page, opt the page into dynamic rendering by reading the searchParams prop" — confirms the segment-level dynamic marking behavior

## Confidence

**Medium** — The segment-level dynamic marking is confirmed by Next.js documentation and the import of `PrerenderStore*` types. The exact detection code path was not read from source.
