# BUG-07 Baseline — Axios mergeConfig loses Authorization header

## Root cause

**H2 and H3 are both contributing, with H2 being primary.** From `lib/core/Axios.js` `_request`:

```js
let contextHeaders = headers && utils.merge(headers.common, headers[config.method]);

headers && utils.forEach(methodList.concat('common'), (method) => {
  delete headers[method];
});

config.headers = AxiosHeaders.concat(contextHeaders, headers);
```

This flattening runs on the **merged** config (after `mergeConfig(this.defaults, config)` ran). The `headers` variable here is the merged headers object from `mergeConfig`. `mergeConfig` correctly deep-merges plain objects, so `defaults.headers.common` and `request.headers.common` should merge.

The actual loss happens at `config.headers = AxiosHeaders.concat(contextHeaders, headers)` — at this point `headers` no longer has `.common` (deleted above), but `contextHeaders` contains the merged common headers. If `contextHeaders` is `null` or `undefined` (because `headers` was falsy or the method didn't match), the `concat` call only receives the request's flattened headers without defaults.

More specifically: if a request config is provided **without** a `headers` object at all, `mergeConfig` picks `defaults.headers` as-is. But if the request config provides `headers: {}` (empty), `mergeConfig.getMergedValue` calls `utils.merge({}, defaults.headers)` — this shallow-merges `defaults.headers.common` into the result. Then the deletion step removes `.common`, and if the method key matches nothing, `contextHeaders = utils.merge(undefined, undefined)` → `undefined`. So `AxiosHeaders.concat(undefined, {})` produces an empty headers object, losing `Authorization`.

## Exact source location

- `lib/core/Axios.js`: header flattening at `let contextHeaders = headers && utils.merge(headers.common, headers[config.method])`
- `lib/core/mergeConfig.js`: `getMergedValue` uses `utils.merge({caseless}, target, source)` — correctly deep-merges but doesn't guard against empty `headers` objects

## Fix proposal

In the `_request` flattening, guard the merge against undefined values and explicitly include `common` in all cases:

```js
let contextHeaders = AxiosHeaders.from(headers && utils.merge(
  headers.common || {},
  headers[config.method] || {}
));
```

And ensure `mergeConfig` preserves `common`/`defaults` headers when the request provides an empty `headers: {}` object rather than treating it as a full override.

## Evidence

- `lib/core/Axios.js`: header flattening code confirmed via `ghGetFileContent`
- `lib/core/mergeConfig.js`: `getMergedValue` confirmed — uses `utils.merge.call({caseless}, target, source)`

## Confidence

**Medium** — the mechanism is clear in source but the exact trigger conditions (empty headers object vs missing headers) need a specific repro to confirm the exact code path.
