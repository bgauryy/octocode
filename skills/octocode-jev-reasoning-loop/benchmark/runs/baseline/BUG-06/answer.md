# BUG-06 Baseline — Axios synchronous request interceptor rejection bypasses nothing

## Root cause

**H1 is correct, with a nuance.** Looking at `lib/core/Axios.js`, the synchronous request interceptor path:

```js
while (i < len) {
  const onFulfilled = requestInterceptorChain[i++];
  const onRejected = requestInterceptorChain[i++];
  try {
    newConfig = onFulfilled ? onFulfilled(newConfig) : newConfig;
  } catch (error) {
    if (!onRejected) {
      promise = Promise.reject(error);
      break;
    }
    try {
      const rejectedResult = onRejected.call(this, error);
      if (utils.isThenable(rejectedResult)) {
        promise = Promise.resolve(rejectedResult).then(() => dispatchRequest.call(this, newConfig));
      }
    } catch (rejectedError) {
      promise = Promise.reject(rejectedError);
    }
    break;
  }
}
```

After the break, execution falls through to:
```js
i = 0;
len = responseInterceptorChain.length;
while (i < len) {
  promise = promise.then(responseInterceptorChain[i++], responseInterceptorChain[i++]);
}
```

So ALL response interceptors are chained onto the rejected promise. Their `fulfilled` handlers won't be called (promise is rejected), but their `rejected` handlers WILL be called in sequence. This is the bug: a request interceptor failure routes through response interceptors' `rejected` handlers, which is semantically wrong — response interceptors should only handle responses, not request failures.

Additionally, the `onRejected` handler recovery path (when it returns a thenable) incorrectly chains `dispatchRequest` even when the original request context (`newConfig`) may be in an invalid state.

## Exact source location

`lib/core/Axios.js` — `_request`, synchronous path, lines ~215–280. The response interceptor loop at the end runs unconditionally after the request loop, not guarded by whether the request interceptors succeeded.

## Fix proposal

Guard the response interceptor chain construction on whether the request phase succeeded:

```js
if (!promise) {
  try {
    promise = dispatchRequest.call(this, newConfig);
  } catch (error) {
    promise = Promise.reject(error);
  }
}

// Only attach response interceptors if request completed (not if it threw)
i = 0;
len = responseInterceptorChain.length;
while (i < len) {
  promise = promise.then(responseInterceptorChain[i++], responseInterceptorChain[i++]);
}
```

To fully fix this, the response interceptors should be skipped when `promise` was set to a rejected state from a request interceptor (add a `requestFailed` flag checked before chaining response interceptors).

## Evidence

- `lib/core/Axios.js` synchronous path — break+fall-through to response loop confirmed via `ghGetFileContent`
- `lib/core/InterceptorManager.js` — interceptor handler structure confirmed: `{fulfilled, rejected, synchronous, runWhen}`

## Confidence

**High** — the control flow is deterministic and the fall-through is explicit in source.
