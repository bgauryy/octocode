# BUG-01 Treatment — Next.js parallel routes @slot stale content

## THINK

**Source observations:**
- `app-render.tsx` imports `FlightRouterState`, `NavigationFlightResponse`, `DynamicNavigationFlightResponse` — confirms segment tree structure exists
- Router cache key construction: inaccessible (router-reducer files redirect-blocked for vercel/next.js at path depth)
- App Router architecture: Router Cache is segment-tree-keyed; @slot segments are tracked separately from primary segments

**Three competing causes:**
- H1: Router Cache key for @slot excludes sibling segment params → cache hit on slot despite sibling changing
- H2: FlightDataRenderer reuses previous slot RSC payload because layout doesn't re-render
- H3: Parallel route tree diff treats unchanged @slot paths as stable → skip re-fetch

**Uncertainty:** H1, H2, H3 are all plausible from architecture. The router-reducer source is the discriminating evidence — not accessible.

## GATE

**Classification: `missing_fact`** — the discriminating evidence (router cache key construction in `router-reducer/reducers/`) is inaccessible via the available tools (redirect-blocked). Jev call would be premature — a judgment on incomplete evidence risks reinforcing a wrong hypothesis.

**Action:** Do NOT call Jev. Flag the evidence gap explicitly.

## Root cause (best available reasoning)

**H1 most likely** based on App Router architecture knowledge and the confirmed import of `FlightRouterState` (which encodes the segment tree). The Router Cache uses the FlightRouterState tree as the keying structure, and @slot paths are tracked as sibling branches. If the cache key for a @slot includes only the slot's own segment but not the primary dynamic segment's current value, the cache hit check incorrectly reuses stale slot content after a sibling navigation.

This matches the reported symptom exactly: @slot renders stale content after navigating between sibling pages where only the primary segment changes.

## Fix (conditional on H1)

In `packages/next/src/client/components/router-reducer/`, the Router Cache tree traversal should include the current primary segment value in the cache key hash for all sibling parallel routes:

```
key(slot) = hash(primarySegment, slotName, slotSegmentPath)
```

rather than:
```
key(slot) = hash(slotName, slotSegmentPath)
```

## Evidence gap

The router-reducer source at `packages/next/src/client/components/router-reducer/` was not accessible (GitHub redirect denied for vercel/next.js). H1 is the most architecturally grounded hypothesis but is **not source-confirmed**.

## Decision snapshot
- **GATE:** `missing_fact` — Jev call deferred
- **Jev calls:** 0 (insufficient evidence for useful judgment)
- **Confidence:** Medium — architectural reasoning, not source-confirmed
