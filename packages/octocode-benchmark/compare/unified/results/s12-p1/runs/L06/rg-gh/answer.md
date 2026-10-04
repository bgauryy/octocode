**Answer:** `generateEtags` (default `true`) controls whether Next.js adds an `ETag` header to rendered page responses and answers conditional requests with `304`. It also controls whether static files get ETags. I did not read `lib/etag.ts` or run anything.

**Config**
- The option is declared at `server/config-shared.ts:1824`, defaulted to `true` at `:2213`, and validated in `server/config-schema.ts:670`.
- It is copied into the render options at `config-shared.ts:2614`. `base-server.ts:540` and `:577` then pass it into `renderOpts`.

**Rendered HTML and data responses**
- `sendRenderResult` in `server/send-payload.ts:35` does the work.
- The check is at `send-payload.ts:66`: `if (generateEtags && payload !== null)`.
- When it passes, it computes `generateETag(payload)` and calls `sendEtagResponse`.
- `sendEtagResponse` (`send-payload.ts:11-26`) sets the `ETag` header at `:23`. It then uses `fresh(req.headers, { etag })` at `:26`, which matches the request's `If-None-Match`. On a match it returns early with a `304`, so no body is sent.
- When `generateEtags` is false, none of this happens. There is no `ETag` header and no `304` from this path. The response still gets `Content-Length` and the body.
- Dynamic results have `payload === null` (`send-payload.ts:63`), so they never get an ETag, whatever the setting. Streamed responses are in that group.

**Callers that pass the option into `sendRenderResult`**
- `base-server.ts:2152` and `:2171`, for the generic server path.
- `route-modules/pages/pages-handler.ts:739`, for the Pages router.
- `build/templates/app-page-runtime.ts`, at about ten sites starting at `:1804`, for App Router pages.

**Static files**
- In `server/lib/router-server.ts:665`, `serveStatic(...)` receives `etag: config.generateEtags`. The comment there reads "Ensures that etags are not generated for static files when disabled."
- `serveStatic` serves files through the `send` module. Its `etag` option turns the file ETag header on or off, so the setting also decides whether static files get an `ETag`.

**Uncertainty:** I didn't open the `send` module to confirm its `etag` semantics. That part comes from the option name and the code comment.