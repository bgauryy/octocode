# BUG-05 Baseline — Axios response interceptor error swallow

## Root cause

**H1 is correct.** In `lib/core/Axios.js`, the async interceptor chain is built as a flat array and consumed with:

```js
while (i < len) {
  promise = promise.then(chain[i++], chain[i++]);
}
```

The chain is `[dispatchRequest, undefined, resp_fulfilled_1, resp_rejected_1, resp_fulfilled_2, resp_rejected_2, ...]`.

Each `.then(fulfilled, rejected)` call means `rejected` handles rejections from **the incoming promise** — NOT from errors thrown inside `fulfilled`. When `resp_fulfilled_1` throws, that throw creates a **new** rejected promise, which is handled by the next pair's rejected handler (`resp_rejected_2`), or goes unhandled if the throwing interceptor is last in the chain.

In the specific scenario where a prior interceptor (`resp_fulfilled_1`) returns a modified response object (succeeds), then `resp_fulfilled_2` is called with that value. If `resp_fulfilled_2` throws, `resp_rejected_2` is the handler for the `.then(resp_fulfilled_2, resp_rejected_2)` call — but `resp_rejected_2` handles rejections from the PREVIOUS promise (the result of the prior `.then`), not from `resp_fulfilled_2`'s throw.

So the throw propagates unhandled through all remaining chain pairs, eventually becoming an unhandled promise rejection that some environments (Node.js with `unhandledRejection: ignore`) swallow silently.

## Exact source location

`lib/core/Axios.js` — `_request` method, async branch:
```js
// Line ~200
const chain = [dispatchRequest.bind(this), undefined];
chain.unshift(...requestInterceptorChain);
chain.push(...responseInterceptorChain);
len = chain.length;

promise = Promise.resolve(config);

while (i < len) {
  promise = promise.then(chain[i++], chain[i++]);  // ← the bug is here
}
```

The synchronous path at lines ~215–260 wraps fulfilled calls in try/catch and routes to rejected explicitly — the async path lacks this safety.

## Fix proposal

Wrap each fulfilled and rejected call in the async chain with proper error routing. The cleanest fix is to process pairs explicitly rather than using the flat array:

```js
// In the async chain-building loop, replace:
promise = promise.then(chain[i++], chain[i++]);

// With:
const onFulfilled = chain[i++];
const onRejected = chain[i++];
promise = promise.then(
  onFulfilled
    ? (val) => { try { return onFulfilled(val); } catch(e) { return onRejected ? onRejected(e) : Promise.reject(e); } }
    : undefined,
  onRejected
);
```

Alternatively, re-use the synchronous path's explicit try/catch structure for the async chain as well.

## Evidence

- `lib/core/Axios.js`: async chain loop confirmed at lines 196–210 via `ghGetFileContent`
- `lib/core/Axios.js`: synchronous path try/catch at lines 215–260 — confirms the discrepancy
- JavaScript Promise spec: `.then(onFulfilled, onRejected)` — `onRejected` does not catch errors thrown in `onFulfilled` of the same call

## Confidence

**High** — the mechanism is deterministic and directly visible in source. The async vs synchronous path discrepancy is explicit.
