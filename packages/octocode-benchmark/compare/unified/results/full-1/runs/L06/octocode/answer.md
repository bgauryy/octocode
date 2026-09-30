`generateEtags` (default `true`) controls whether Next.js adds an `ETag` header to rendered pages and to static files. When it is `false`, Next.js skips that header and the 304 handling that goes with it. I read all of this in the source and did not run it.

**Config**
- `server/config-shared.ts:1824` declares it as an optional boolean, and `server/config-shared.ts:2213` sets the default to `true`.
- `server/config-schema.ts:670` validates it as `z.boolean().optional()`.
- `server/config-shared.ts:2614` copies it into the render options, and the type is at `server/config-shared.ts:2452`.

**Rendered pages and data responses**
- In `server/base-server.ts:540-577`, the constructor reads `generateEtags` from `nextConfig` and passes it into `renderOpts`.
- `server/next-server.ts:554` and `:563` pass it along as well.
- `sendResponse` in `server/base-server.ts:2152-2171` reads it from `this.renderOpts` and hands it to `sendRenderResult`.
- The Pages Router handler calls `sendRenderResult` directly with `nextConfig.generateEtags` (`server/route-modules/pages/pages-handler.ts:739`).
- The effect is in `sendRenderResult` (`server/send-payload.ts:66-71`):
  - If `generateEtags` is true and the payload is non-null, it calls `generateETag(payload)`. It then calls `sendEtagResponse(req, res, etag)` and returns early if that returns true.
  - The payload is `result.toUnchunkedString()` for non-dynamic results. For dynamic (streamed) results it is `null` (`server/send-payload.ts:63`), so no ETag is produced even when the option is on.
  - When the option is false, this block is skipped. The response then goes out with its Content-Type and Content-Length, with no ETag header and no 304 short-circuit.
- I did not open `generateETag` or `sendEtagResponse`. I'm inferring from the names and the early return that `sendEtagResponse` sets the `ETag` header and returns true when the request's conditional headers match, so it sends a 304.

**Static files**
- `server/lib/router-server.ts:665` passes `etag: config.generateEtags` to `serveStatic`, with the comment "Ensures that etags are not generated for static files when disabled."
- I didn't confirm that `serveStatic` is the `send` package. The comment at `router-server.ts:670-673` refers to the `send` module, and `etag` is an option that `send` supports.

**Uncertainty**
- The first search was truncated (`isPartial`), so there may be other usages I didn't see. I did not check `generateETag` or `sendEtagResponse`, or how the app-router paths use the option beyond `base-server.ts`.