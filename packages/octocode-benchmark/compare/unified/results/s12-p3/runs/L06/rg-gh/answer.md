**Short answer:** `generateEtags` (default `true`) decides whether Next.js adds an `ETag` header to rendered page and RSC responses, and whether it can answer a matching conditional request with `304`. It also turns the `etag` option on or off for static files served through `serveStatic`.

**Config definition**
- It is declared as `generateEtags?: boolean` at `packages/next/src/server/config-shared.ts:1824`.
- The default is `true` at `config-shared.ts:2213`.
- The zod schema accepts an optional boolean at `config-schema.ts:670`.

**Path 1: rendered pages and RSC payloads (`sendRenderResult`)**
- `BaseServer` copies the option into `renderOpts` at `base-server.ts:540` and `:577`.
- In the response path it reads `generateEtags` from `renderOpts` at `base-server.ts:2152`. It then passes the value to `this.sendRenderResult(...)` at `:2171`.
- `next-server.ts:559-563` forwards it to `sendRenderResult` in `send-payload.ts`.
- The pages handler passes it at `route-modules/pages/pages-handler.ts:739`. The app-page template passes it in many calls, for example `build/templates/app-page-runtime.ts:1804` and `:1822`.
- The effect is in `send-payload.ts:66-71`:
  - If `generateEtags && payload !== null`, it computes `generateETag(payload)`.
  - It then calls `sendEtagResponse`.
  - `sendEtagResponse` sets the `ETag` header (`send-payload.ts:11-21`).
  - If `fresh(req.headers, { etag })` says the client's copy is current, it sets status `304` and ends the response with no body.
  - Otherwise the normal response continues, with the `ETag` header already set.
- `payload` is `null` for dynamic or streamed results (`send-payload.ts:64`: `result.isDynamic ? null : result.toUnchunkedString()`). Those never get an ETag, whatever the setting.
- When the option is `false`, none of this runs. No `ETag` is set, no `304` is produced, and the response goes out with `Content-Type` and `Content-Length`, or is piped if dynamic.

**Path 2: static files**
- In `lib/router-server.ts:662-666`, `serveStatic(..., { etag: config.generateEtags })` carries the comment "Ensures that etags are not generated for static files when disabled."
- `serveStatic` is the `send`-style file server, so this switches its own ETag generation on or off. I did not read its internals.

**Uncertainty:** I did not trace `generateETag` in `lib/etag` or the `send` library's ETag format. I also did not check whether other call sites, such as route handlers or image optimization, use their own ETag logic independent of this flag.