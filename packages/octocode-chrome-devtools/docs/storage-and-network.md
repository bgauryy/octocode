# Storage and network capture recipes

Three `check` recipes capture what a page stores and what it sends over the wire. Each writes evidence files under the run folder and prints `[ARTIFACT]` lines; stdout stays small and never carries a secret value.

| Recipe | Question it answers | Options |
|---|---|---|
| `storage-dump` | What does this origin keep in the browser? | `STORAGE_DUMP_URL`, `STORAGE_DUMP_INCLUDE_VALUES`, `STORAGE_DUMP_IDB_RECORDS` |
| `network-events` | Which WebSocket frames, SSE messages and HTTP exchanges happen in a window? | `NETEVENTS_URL`, `NETEVENTS_MS` (required), `NETEVENTS_MATCH`, `NETEVENTS_KINDS` |
| `network-conditions` | How does the page behave offline, throttled, blocked or against a mocked API? | `NETCOND_URL` (required), `NETCOND_OFFLINE`, `NETCOND_LATENCY_MS`, `NETCOND_DOWN_KBPS`, `NETCOND_UP_KBPS`, `NETCOND_BLOCK_URLS`, `NETCOND_EXTRA_HEADERS`, `NETCOND_MOCKS` |

Booleans are `true`/`false` (the engine passes `1`/`0`); arrays and objects are JSON values.

## storage-dump

Dumps the current page's origin (after loading `STORAGE_DUMP_URL` when given): localStorage and sessionStorage, IndexedDB (databases, object stores, key paths, indexes, exact entry counts), CacheStorage (cache names, request URL, method, status, header names), service worker registrations and cookies (name, domain, path, secure, httpOnly, sameSite, expires, size, partitionKey).

- Values are redacted by default: each becomes `{"redacted":true,"type":"string","bytes":21}`. Keys, names and flags stay visible.
- IndexedDB records are saved only with `STORAGE_DUMP_IDB_RECORDS=N`: the first N per store in key order (their values redacted too). Counts are always exact.
- `STORAGE_DUMP_INCLUDE_VALUES=true` writes values (storage values, cookie values, IndexedDB record values, cache header values) to `storage-dump.json` only, mode `0600`. Stdout never carries them; it prints `[FINDING] SECRET_VALUES_SAVED {"file":...}`. Treat that file as a secret.
- An area Chrome refuses is listed in `errors[]` and as `[FINDING] STORAGE_DUMP_UNAVAILABLE`; the others still save and the exit code is 1.

```
[METRIC] STORAGE_DUMP origin=http://127.0.0.1:3000 localStorage=1 sessionStorage=1 idbDatabases=1 idbStores=1 idbEntries=5 idbRecordsSaved=0 caches=1 cacheEntries=1 serviceWorkers=0 cookies=1 valuesRedacted=true
[ARTIFACT] STORAGE_DUMP .../storage-dump.json
```

## network-events

Attaches listeners before the optional load, then observes for `NETEVENTS_MS`. A passive window on the current tab sees only sockets and requests that start inside it (Chrome does not replay `webSocketCreated` for earlier sockets).

- `network-events.jsonl`: one event per line (`n`, `t` ms since start, `kind`, `type`, `requestId`, `url`, plus fields per type). Kinds: `websocket` (created, handshakeRequest, handshakeResponse, frame-sent, frame-received, frame-error, closed), `sse` (message), `http` (request, request-extra, response, response-extra, finished, failed; with initiator and timing).
- `payloads/<n>.txt` (text) or `.bin` (binary frames, decoded) hold each frame or SSE message whole; the event line names it in `payloadFile` and `payloadBytes`. Nothing is truncated.
- `NETEVENTS_MATCH` keeps events whose URL contains the substring; `NETEVENTS_KINDS` is a JSON array of `websocket|sse|http`. `filteredOut` in the metric counts events dropped by the filter.
- Header values for `Authorization`, `Cookie`, `Set-Cookie` and `Proxy-Authorization` are saved and printed as `[redacted N chars]`; names stay. **Payload sidecars are raw evidence and may contain tokens**, like HAR bodies; handle them as secrets.
- Stdout shows counts per kind and the first 10 events, then a `[NEXT]` call that pages the jsonl with `evidence-query` (`--format jsonl --cursor 10`). Add `--where` (for example `kind=websocket`) to filter.

## network-conditions

Applies every condition in one session, then loads `NETCOND_URL`: `Network.emulateNetworkConditions` (offline, latency, kbps converted to bytes/s), `Network.setBlockedURLs`, `Network.setExtraHTTPHeaders`, and `Fetch` interception for mocks.

- `NETCOND_MOCKS`: `[{match, status, headers?, body?}]`. `match` is a URL glob when it contains `*`, otherwise a substring; the first matching mock answers, other requests continue untouched. `status` is 100-599 and `match` is non-empty. A bad option exits 2 with `[FINDING] NETCOND_INVALID` before the page is touched.
- `NETCOND_BLOCK_URLS`: URL patterns with `*` wildcards.
- Each request is reported as `{url, method, outcome, status, bytes, error?}` with outcome `ok`, `blocked`, `mocked`, `failed`, `failed-offline` or `pending`. The page result is `loadEventReached`, `navigationError`, `consoleErrors`, `pageExceptions`. Offline, the navigation itself fails and `loadEventReached` is false.
- The load waits for the load event, then for the network to go quiet; a page that never finishes loading ends with the run's own limit.
- **The conditions end when the session closes.** Output includes `[NOTE] CONDITIONS_END_WITH_SESSION`; later calls run unthrottled and unmocked. The CDP websocket the recipe uses is not page traffic and is never blocked or mocked.
- `network-conditions.json` has every row; stdout shows the first 10 and a `[NEXT]` evidence-query call for the rest.
