# BUG-05 Treatment — Axios response interceptor error swallow

## THINK

**Observations from source investigation:**
- Async chain loop: `while (i < len) { promise = promise.then(chain[i++], chain[i++]); }` — no try/catch around `fulfilled`
- Sync path: `try { newConfig = onFulfilled(newConfig); } catch(error) { ... onRejected(error) ... }` — explicit routing
- settle.js: called inside adapter before interceptor chain — cannot mask interceptor throws
- JS spec: `.then(fulfilled, rejected)` — `rejected` handles incoming rejections, not errors thrown inside `fulfilled`

**Competing hypotheses:**
- H1: async `.then` pair semantics — fulfilled throw creates new rejection not caught by paired rejected
- H2: sync/async discrepancy — same root as H1 framed differently (fix location is the async loop)
- H3: settle.js masks 2xx — **ruled out**: settle.js runs before response interceptors

**Uncertainty:** H1 and H2 describe the same code path. Is H3 plausible enough to withhold conclusion?

## GATE

**Classification: `disputed_inference`** — evidence is complete (all source read, JS spec confirmed), the remaining dispute is purely interpretive: whether H3 is conclusively ruled out by settle.js position.

Jev call warranted: Yes. `willChangeAction=true` — Jev may confirm H1 over H3, shifting the fix from settle.js to the async loop.

## JEV RESULT

```json
{
  "claim_status": "supported",
  "claim_status_probability": 0.99,
  "decisive_basis": "B1",
  "decisive_basis_probability": 0.98
}
```

**Claim: supported at 0.99.** H1 is correct. Decisive basis B1: JS Promise `.then(fulfilled, rejected)` — fulfilled throw is not caught by the paired rejected handler.

**Action:** H3 ruled out with 0.99 confidence. Fix location is the async while loop.

## Root cause

**H1 confirmed.** The async interceptor chain at `lib/core/Axios.js` consumes pairs via `promise.then(chain[i++], chain[i++])` without a try/catch around the fulfilled call. Per JavaScript Promise semantics, a throw inside `fulfilled` creates a new rejected promise — it is NOT caught by the `rejected` handler in the same `.then` call. The throw propagates as an unhandled rejection to the next pair's rejected, or goes unhandled if the throwing interceptor is last.

The synchronous path at lines ~215–260 correctly wraps fulfilled in try/catch and explicitly routes errors to the paired rejected handler — confirming the async path is the specific site of the bug.

## Fix

In the async chain loop, replace:
```js
promise = promise.then(chain[i++], chain[i++]);
```
With explicit try/catch routing matching the sync path's behavior:
```js
const onFulfilled = chain[i++];
const onRejected = chain[i++];
promise = promise.then(
  onFulfilled ? (val) => { try { return onFulfilled(val); } catch(e) { return onRejected ? onRejected(e) : Promise.reject(e); } } : undefined,
  onRejected
);
```

## Decision snapshot
- **Before Jev:** H1 likely (0.8 confidence), H3 possible (0.2)
- **After Jev:** H1 confirmed (0.99), H3 ruled out. Direction unchanged, confidence threshold crossed for assertion.
