`generateEtags` (default `true`) affects HTTP responses in two places. One is the ETag and conditional-request handling for rendered pages. The other is the ETag for static files served by the router.

**Config plumbing**
- `config-shared.ts:1824` declares the option, `config-schema.ts:670` validates it as a boolean, and `config-shared.ts:2213` sets the default to `true`.
- `config-shared.ts:2614` copies it into the render options, which `base-server.ts:540` and `base-server.ts:577` read from `nextConfig`. `next-server.ts:563` also passes it along.

**Effect 1: rendered pages and data responses**
- `base-server.ts:2152` reads `generateEtags` from `this.renderOpts`. `base-server.ts:2171` passes it to `sendRenderResult`.
- In `send-payload.ts:66`, the condition is `if (generateEtags && payload !== null)`.
  - When true, it computes `generateETag(payload)` and calls `sendEtagResponse(req, res, etag)`. If that returns true, it returns without sending the body.
  - The body of `sendEtagResponse` is outside the lines I read. Its name and this early return suggest it sets the `ETag` header and answers a matching conditional request with a 304, but I didn't confirm that.
  - When false, no ETag is generated and no conditional-request short-circuit happens. The full body is sent with `Content-Length` (`send-payload.ts:76-78`).
- `payload` is null when `result.isDynamic` is true (`send-payload.ts:63`). Those responses are streamed through `pipeToNodeResponse` and never get an ETag, whatever the setting.
- `pages-handler.ts:739` passes `nextConfig.generateEtags` on the pages route-module path, which I did not read.

**Effect 2: static files**
- In `router-server.ts:663-665`, `serveStatic(...)` gets `etag: config.generateEtags`. The comment there says "Ensures that etags are not generated for static files when disabled". `serveStatic` is the `send` module's option, and I didn't read its implementation.

**Uncertainty:** I didn't read `generateETag` or `sendEtagResponse` (the `send-payload.ts` lines before 30), so the exact header and 304 behavior is inferred from the names and call flow.