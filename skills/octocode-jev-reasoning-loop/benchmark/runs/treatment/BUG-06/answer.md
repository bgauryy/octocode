# BUG-06 Treatment — Axios sync request interceptor rejection calls response interceptors

## THINK

**Source observations:**
- Synchronous path: `while (i < len) { try { newConfig = onFulfilled(newConfig); } catch (error) { ... break; } }`
- After `break`, execution continues to: `i = 0; len = responseInterceptorChain.length; while (i < len) { promise = promise.then(responseInterceptorChain[i++], responseInterceptorChain[i++]); }`
- The response interceptor loop is unconditional — it runs regardless of whether the request phase succeeded

**Three causes:**
- H1: After request interceptor throws and break fires, response interceptors are still chained onto the rejected promise
- H2: Sync path bug in loop variable `i` starting at non-zero index for response interceptors
- H3: `legacyInterceptorReqResOrdering` interleaves request/response into single chain

**Analysis:** H1 is directly confirmed by reading the `_request` synchronous path — the `break` exits the request loop but the response loop runs unconditionally with `i = 0`. H2 is incorrect (i is reset to 0 explicitly). H3 only applies when the transitional flag is set.

## GATE

**Classification: `deterministic`** — the unconditional response interceptor loop after the request loop is directly visible in source code. The behavior is mechanistically certain. No Jev needed.

## Root cause

**H1 confirmed by source.** In `lib/core/Axios.js`, the synchronous path's response interceptor chain is appended unconditionally after the request interceptor loop exits:

```js
// Reset i regardless of how request loop exited
i = 0;
len = responseInterceptorChain.length;
while (i < len) {
  promise = promise.then(responseInterceptorChain[i++], responseInterceptorChain[i++]);
}
```

When a request interceptor throws, `promise = Promise.reject(error)` is set and the request loop breaks. The response interceptor loop then chains all response interceptors onto this rejected promise. Response interceptors' `fulfilled` handlers are skipped (promise is rejected), but `rejected` handlers are called — semantically incorrect since no HTTP response exists.

## Fix

Add a guard before the response interceptor loop:

```js
if (!requestFailed) {
  i = 0;
  len = responseInterceptorChain.length;
  while (i < len) {
    promise = promise.then(responseInterceptorChain[i++], responseInterceptorChain[i++]);
  }
}
```

Where `requestFailed` is set to `true` when the request interceptor loop sets `promise = Promise.reject(...)`.

## Decision snapshot
- **GATE:** `deterministic` — no Jev call
- **Jev calls:** 0
- **Confidence:** High — source directly confirms H1
