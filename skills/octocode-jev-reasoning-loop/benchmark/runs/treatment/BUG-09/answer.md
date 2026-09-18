# BUG-09 Treatment — Vite HMR queueUpdate wrong callback order

## THINK

**Source observations:**
- `queueUpdate`: pushes `fetchUpdate(payload)` promise immediately, then `if (!pendingUpdateQueue) { pendingUpdateQueue = true; await Promise.resolve(); pendingUpdateQueue = false; const loading = [...updateQueue]; ... await Promise.all(loading) }`
- Flag set to `false` BEFORE `await Promise.all(loading)` — window exists for concurrent flush
- `Promise.all` preserves input array order → within a single flush, callbacks are correctly ordered
- `hmrHandler` outer queue serializes WS messages sequentially → two updates CAN enter `queueUpdate` in order

**Competing causes:**
- H1: Promise.all resolves in network-timing order (callbacks ordered by network)
- H2: pendingUpdateQueue race — flag cleared before Promise.all, second flush starts concurrently

**Discrimination:** Promise.all preserves order → H1 is false per spec. H2 is confirmed by reading the flag lifecycle.

## GATE

**Classification: `disputed_inference`** — H1 is plausible to someone unfamiliar with Promise.all ordering. H2 requires combining the flag lifecycle with Promise.all spec knowledge.

Jev call warranted: Yes. `willChangeAction=true` — confirms H1 vs H2 changes the fix location.

## JEV RESULT

```json
{
  "claim_status": "supported",
  "claim_status_probability": 0.87,
  "decisive_basis": "B1",
  "decisive_basis_probability": 0.93
}
```

**Claim: supported at 0.87.** pendingUpdateQueue timing race is confirmed. Decisive basis B1: `pendingUpdateQueue=false` before `Promise.all` + outer message serialization means a second flush can start concurrently.

## Root cause

**H2 confirmed** (Jev: supported 0.87). The `pendingUpdateQueue = false` line at `packages/vite/src/shared/hmr.ts` executes before `await Promise.all(loading)` completes. When a rapid second file save triggers a new `queueUpdate` call during the first flush's Promise.all await, `pendingUpdateQueue` is already `false`, so a new independent flush cycle starts. Two flush cycles run concurrently — whichever set of `fetchUpdate` promises resolves first wins, causing callbacks to fire out of arrival order.

Promise.all itself is order-preserving per spec, so H1 is incorrect — the ordering break is purely the concurrent flush cycles.

## Fix

Move `pendingUpdateQueue = false` to after `Promise.all` completes:

```ts
public async queueUpdate(payload: Update): Promise<void> {
  this.updateQueue.push(this.fetchUpdate(payload))
  if (!this.pendingUpdateQueue) {
    this.pendingUpdateQueue = true
    await Promise.resolve()
    const loading = [...this.updateQueue]
    this.updateQueue = []
    try {
      ;(await Promise.all(loading)).forEach((fn) => fn && fn())
    } finally {
      this.pendingUpdateQueue = false
      if (this.updateQueue.length > 0) this.queueUpdate(this.updateQueue[0])
    }
  }
}
```

## Decision snapshot
- **Before Jev:** H2 likely (0.75), H1 possible (0.20), H3 possible (0.05)
- **After Jev:** H2 confirmed (0.87), H1 ruled out (Promise.all is order-preserving)
- **Direction changed:** No — H2 was already the lead; Jev raised confidence above assertion threshold
