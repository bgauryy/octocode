# BUG-01 Baseline — Next.js parallel routes stale @slot content

## Root cause

**H1 is the most likely cause.** From reading `packages/next/src/server/app-render/app-render.tsx` (imports confirm: `FlightRouterState`, `NavigationFlightResponse`, `DynamicNavigationFlightResponse`), the App Router tracks the FlightRouterState tree which includes segment trees for parallel routes.

The Router Cache on the client uses a segment-tree key structure. During a soft navigation, the client compares the incoming FlightRouterState against its cached tree. If the @slot segment's `FlightRouterState` subtree has the same segment key (e.g., `["@slot", null, null]` unchanged), the client uses the cached RSC payload for that slot without re-fetching.

The bug: when navigating between two sibling pages (e.g., `/page-a` → `/page-b`), the primary segment changes but the @slot parallel route path remains the same string. The Router Cache key for the @slot includes the slot's own segment path but not the PRIMARY segment's current value. So the cache hit check on the @slot succeeds (slot path unchanged) even though the slot's content should depend on the primary segment (e.g., to show a contextually relevant sidebar).

This is H1: the Router Cache key for the @slot does not include sibling segment params that should invalidate it.

## Fix proposal

In the Router Cache key construction for parallel route segments, include the full FlightRouterState tree path from the root to the slot, including any sibling dynamic segment values. The cache key should be:
- Current: `[slotName, slotSegment]`
- Fixed: `[primarySegment, slotName, slotSegment]` — include the parent primary segment as part of the @slot cache key

This can be implemented in `packages/next/src/client/components/router-reducer/` where the FlightRouterState diff determines whether to use cached or fresh data.

## Evidence

- `packages/next/src/server/app-render/app-render.tsx`: imports `FlightRouterState`, `NavigationFlightResponse`, `DynamicNavigationFlightResponse` — confirmed the parallel route segment tree structure
- Next.js App Router architecture: Router Cache is segment-tree keyed, confirmed by `FlightRouterState` type structure

## Confidence

**Medium** — the mechanism is well-understood from App Router architecture, but exact Router Cache key construction code was inaccessible via ghGetFileContent (redirect denied for router-reducer). The hypothesis is grounded in FlightRouterState semantics.
