**Short answer:** `generateEtags` (default `true`) controls whether Next.js adds an `ETag` to rendered HTML/data responses and to static files. When it's on, a request whose `If-None-Match` matches the ETag gets a 304 response. When it's off, neither happens.

**Config:**
- The option is typed at `packages/next/src/server/config-shared.ts:1824` and defaults to `true` at `config-shared.ts:2213`.
- It is validated in `config-schema.ts:670`.
- It is passed into the server's render options at `config-shared.ts:2614`, and again at `base-server.ts:540` and `base-server.ts:577`.
- `next-server.ts:563` also passes it along.

**Effect 1 – rendered responses (`send-payload.ts`):**
- `sendRenderResult` runs at `send-payload.ts:35`.
- `payload` is null for dynamic results, and otherwise the unchunked string (`send-payload.ts:64`).
- At `send-payload.ts:66-71`: `if (generateEtags && payload !== null)`, it computes `generateETag(payload)` and calls `sendEtagResponse(req, res, etag)`. If that returns true, the function returns early. I did not open `sendEtagResponse` or `generateETag`. I'm inferring from their names that they set the `ETag` header and send the 304.
- If `generateEtags` is false, this block is skipped. The response goes out with `Content-Type` and `Content-Length`, with no ETag and no 304 short-circuit (`send-payload.ts:73-89`).
- Streamed or dynamic results (`payload === null`) never get an ETag, even when the option is on.
- Callers: `base-server.ts:2152-2171` reads it from `this.renderOpts` and passes it in. `pages-handler.ts:739` passes `nextConfig.generateEtags` directly.

**Effect 2 – static files (`lib/router-server.ts:662-666`):**
- `serveStatic(req, res, itemPath, { root, etag: config.generateEtags })` passes the option to the `send` library.
- The code comment reads: "Ensures that etags are not generated for static files when disabled."

**Not verified:** I did not check `sendEtagResponse` or `generateETag` internals, or whether the App Router or other response paths (such as route handlers) read this option. Only the grep hits under `packages/next/src` were examined.