# BUG-07 Treatment — Axios mergeConfig loses Authorization header

## THINK

**Source observations (mergeConfig.js):**
- `getMergedValue(target, source)`: if both plain objects → `utils.merge.call({caseless}, target, source)` — deep merge
- If only `source` is plain object → `utils.merge({}, source)` — copies source only
- Headers merge: uses `mergeDeepProperties` strategy for most keys

**Source observations (Axios.js _request):**
- `let contextHeaders = headers && utils.merge(headers.common, headers[config.method])` — flattens common + method headers
- `headers && utils.forEach(methodList.concat('common'), (method) => { delete headers[method]; })` — deletes namespaces
- `config.headers = AxiosHeaders.concat(contextHeaders, headers)` — concat flattened common + remaining

**Two competing causes:**
- H1: mergeConfig shallow-merges headers — the request's headers object entirely replaces defaults.headers
- H2: The _request flattening step loses common headers when request config provides an empty `headers: {}`
- H3: AxiosHeaders.concat last-defined-wins semantics overwrites defaults.headers.common values

**Analysis:** mergeConfig.js source shows it DOES deep-merge plain objects — so H1 (shallow merge) is INCORRECT as described. The actual issue is in the flattening step (H2): when request provides `headers: {}`, mergeConfig merges `defaults.headers` and `{}`, producing a merged headers object. Then `contextHeaders = utils.merge(merged.common, merged[method])`. If the merge of `defaults.headers.common` and `{}` produces `undefined` for `common` (because `{}` has no `common` key), then `utils.merge(undefined, undefined)` → undefined → `AxiosHeaders.concat(undefined, {})` → empty.

## GATE

**Classification: `disputed_inference`** — the exact trigger (when `headers: {}` vs when headers is omitted) requires tracing `utils.merge` behavior with undefined inputs, which wasn't directly read. H2 and H3 both contribute; H1 is partially wrong (mergeConfig DOES deep-merge, but the issue is the empty-headers edge case).

Jev call: **deferred** — the evidence is incomplete (utils.merge source not read). Calling Jev on incomplete evidence would risk reinforcing a partially-incorrect framing.

## Root cause (best available)

**H2 primary, H3 secondary.** The data loss occurs in the `_request` flattening step when:
1. Request provides `headers: {}` (empty object)
2. `mergeConfig` deep-merges `defaults.headers` into `{}` correctly
3. But the flattened `contextHeaders = utils.merge(result.common, result[method])` where result is the merged object — if the merged `common` is undefined (because empty headers object overwrote common), contextHeaders is undefined
4. `AxiosHeaders.concat(undefined, headers)` drops the defaults.headers.common values

## Fix

In `_request` flattening: guard against undefined `common`:
```js
let contextHeaders = headers && utils.merge(
  headers.common || {},  
  headers[config.method] || {}
);
```

## Decision snapshot
- **GATE:** `disputed_inference` — Jev deferred (evidence incomplete)
- **Jev calls:** 0
- **Confidence:** Medium — H1 disproven by source; H2 is the leading mechanism
