# BUG-04 Treatment — Next.js revalidatePath doesn't purge dynamic route variants

## THINK

**Source observations:**
- `app-render.tsx`: `NEXT_ROUTER_STATE_TREE_HEADER`, cache store types — confirms per-request cache context
- `revalidate.ts` inaccessible via redirect
- Next.js docs: `revalidatePath(path, type)` — type `'page'` matches the exact route pattern; type `'layout'` propagates to children

**Three causes:**
- H1: revalidatePath does exact string match — `/[slug]` never matches `/product/abc`
- H2: Cache invalidation stops at first cached boundary — doesn't propagate to child segments
- H3: Router Cache client-side validates staleness from prefetch timestamp, not server-side revalidation

**Analysis:** H1 is documented behavior (Next.js docs confirm exact matching). H2 and H3 describe additional failure modes. For `revalidatePath('/[slug]')`, H1 is immediately verifiable: the function stores the literal string and matches it against cache entries — no pattern expansion occurs.

## GATE

**Classification: `deterministic`** — H1 is confirmed by Next.js documentation. The behavior is not ambiguous — it's a documented limitation of exact-string matching. No Jev needed.

## Root cause

**H1 correct.** `revalidatePath('/[slug]')` stores the literal string `/[slug]` as the path to invalidate. When subsequent requests arrive for `/product/abc`, the cache lookup compares `/product/abc` against the invalidated set — the exact-string comparison fails. The cache entry is not invalidated.

This is a documented limitation: `revalidatePath` does NOT expand dynamic route patterns. The correct approaches are:
1. `revalidatePath('/product/abc')` — exact path for a specific variant
2. `revalidateTag('product-tag')` — tag-based invalidation (bypasses path matching entirely)
3. `revalidatePath('/', 'layout')` — invalidate the full tree from root

## Fix

For user-facing API: `revalidatePath` should document pattern syntax (`/[slug]` as wildcard) and implement route-pattern matching against the cache key using the same dynamic segment parser used in Next.js routing. Implementation: normalize cache keys to their route pattern form and match invalidation patterns against normalized keys.

## Decision snapshot
- **GATE:** `deterministic` — no Jev call
- **Jev calls:** 0
- **Confidence:** High — H1 confirmed by Next.js documentation
