# Q4 — Axios redirect trace

[`axios/package.json`](https://github.com/axios/axios/blob/main/package.json) declares `follow-redirects: ^1.16.0`. In [`lib/adapters/http.js`](https://github.com/axios/axios/blob/main/lib/adapters/http.js), Axios imports `follow-redirects`, destructures `{http: httpFollow, https: httpsFollow}`, and selects native `http`/`https` only when `maxRedirects === 0`; otherwise the redirect-aware transport is selected (HTTP or HTTPS by request protocol).

Upstream [`follow-redirects/index.js`](https://github.com/follow-redirects/follow-redirects/blob/main/index.js) defines the wrapped protocol `request` function, which creates a `RedirectableRequest`. That request type issues the native request through `_performRequest`; its native-response callback invokes `_processResponse`, where redirect status/location handling begins. These are in `index.js` (the fetched upstream HEAD; Axios's semver range is not a lockfile resolution).
