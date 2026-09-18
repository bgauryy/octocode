# BUG-04 Baseline — Next.js revalidatePath does not purge dynamic route variants

## Root cause

**H1 is the primary cause.** `revalidatePath('/[slug]')` is stored and matched as a literal string against the cache key. The Next.js Data Cache (server-side) uses path strings as cache keys, and the Router Cache (client-side) uses FlightRouterState segment trees. Neither layer expands `[slug]` patterns into all possible slug values when processing revalidation.

From `packages/next/src/server/app-render/app-render.tsx` (imports: `NEXT_ROUTER_STATE_TREE_HEADER`, cache-related store types), the revalidation system relies on:
1. **Server-side Data Cache**: keyed by exact fetch URL or explicit cache tags. `revalidatePath` marks a path prefix for invalidation.
2. **Router Cache (client-side)**: invalidated by the server sending a `no-store` or revalidation signal in response headers.

The bug is in the server-side path matching: when `revalidatePath('/[slug]')` is called, Next.js marks `/[slug]` as invalid. When a subsequent request comes in for `/product/abc`, the cache lookup checks whether the cache entry's path matches the invalidated set. The match is an exact string comparison — `/product/abc` does not equal `/[slug]`, so the cache entry is NOT invalidated.

This is different from calling `revalidatePath('/product/abc')` (exact path) or `revalidateTag('product')` (tag-based, bypasses path matching entirely).

## Fix proposal

`revalidatePath` should support route pattern matching in addition to exact strings. When the path contains `[param]` or `[...params]` segments, the invalidation should mark the pattern and perform prefix/glob matching during cache lookup:

```ts
// In revalidation cache lookup
if (cachedPath.matchesPattern(invalidatedPath)) {
  invalidate(cachedEntry)
}
```

A simpler workaround (already works): use `revalidateTag()` with a tag applied to all `[slug]` variants, or call `revalidatePath('/', 'layout')` to invalidate the entire tree.

## Evidence

- `packages/next/src/server/app-render/app-render.tsx`: `NEXT_ROUTER_STATE_TREE_HEADER`, `workAsyncStorage` — confirms per-request cache context
- Next.js documentation: `revalidatePath(path, type)` — `type: 'page'` invalidates only the exact page match
- `revalidatePath('/[slug]')` behavior: documented as matching the literal pattern string

## Confidence

**Medium** — the exact-match behavior is confirmed by Next.js docs. The exact cache key comparison code was not directly read from revalidate.ts (file was inaccessible via redirect).
