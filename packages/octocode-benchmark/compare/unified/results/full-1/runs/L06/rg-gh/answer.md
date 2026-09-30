**Answer:** `generateEtags` (default `true`) controls whether Next.js adds an `ETag` header, and answers conditional requests with `304`, in two places. The first is rendered pages and data responses, which go through `sendRenderResult`. The second is static files served through `serveStatic`.

**Config**
- The option is declared at `packages/next/src/server/config-shared.ts:1824`, defaults to `true` at `:2213`, and is validated as a boolean at `config-schema.ts:670`.
- `config-shared.ts:2614` copies it into the server's render options.

**1. Rendered pages and data**
- `base-server.ts:540` and `:577` put `generateEtags` from `nextConfig` into `renderOpts`.
- `base-server.ts:2152` reads it from `renderOpts`, and `:2171` passes it to `sendRenderResult`.
- The core logic is in `send-payload.ts:66-71`:
  - The payload is `null` for dynamic or streaming results (`send-payload.ts:63`). Only static, non-streaming results have a string payload.
  - If `generateEtags` is true and the payload is non-null, it calls `generateETag(payload)`.
  - It then calls `sendEtagResponse(req, res, etag)`. I did not open that function. Judging by its name and how the result is used, it sets the `ETag` header and returns true when the request's conditional headers match, in which case the function returns early. I did not confirm that from its source.
- If `generateEtags` is false, or the result is dynamic or streamed, no `ETag` is generated. The response is then sent normally with `Content-Length`, or streamed.
- Other callers pass the same flag to `sendRenderResult`:
  - the pages handler at `route-modules/pages/pages-handler.ts:739`
  - the app-page runtime template at `build/templates/app-page-runtime.ts:1804` and several later lines
  - `next-server.ts:554-563`

**2. Static files**
- `lib/router-server.ts:663-665` calls `serveStatic(..., { etag: config.generateEtags })`. The comment there reads "Ensures that etags are not generated for static files when disabled."
- `serveStatic` is the wrapper around the `send` module, whose `etag` option turns its ETag header on or off. I did not open `serve-static.ts` to confirm that mapping.

**Uncertainty:** I did not read `sendEtagResponse` or `generateETag`, or the `serveStatic` implementation. The description of what they do is inferred from their names and call sites.