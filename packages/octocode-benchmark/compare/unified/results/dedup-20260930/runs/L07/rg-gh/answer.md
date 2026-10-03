`redirect()` and `permanentRedirect()` don't write a response. They throw a special error, and the server catches it at the layer that owns the response, then sets the status code and `Location` header there. All paths below are under `packages/next/src/`.

**1. Throwing**
- `client/components/redirect.ts:9-17`: `getRedirectError` creates `new Error('NEXT_REDIRECT')` and sets `error.digest = "NEXT_REDIRECT;<push|replace>;<url>;<status>;"`.
- `redirect()` (`redirect.ts:34-41`) throws it with status 307. Its type defaults to `push` inside a server action and `replace` otherwise.
- `permanentRedirect()` (`redirect.ts:59-64`) throws it with status 308. Its type defaults to `replace`.
- The status values come from the `RedirectStatusCode` enum: 303, 307 and 308 (`client/components/redirect-status-code.ts`).
- `isRedirectError` (`client/components/redirect-error.ts:19-42`) recognises the error by its digest. `getURLFromRedirectError` and `getRedirectStatusCodeFromError` (`redirect.ts:~70-93`) parse the URL and status back out of it.

**2. Catching and turning it into HTTP** (depends on the context)
- **Route handlers** (`route.ts`): `handleHandlerError` in `server/route-modules/app-route/module.ts:417-451` returns `new Response(null, { status, headers })`.
  - `Location` is the URL, and mutable cookies are appended.
  - The status is 303 if `actionStore.isAction`. Otherwise it is the digest's status (307 or 308).
- **Pages and server components, RSC stream errors**: `server/app-render/app-render.tsx:4386-4400`, in `renderToStream` (function starts at line 3527).
  - It sets `res.statusCode` from the digest and `metadata.statusCode`.
  - It sets `location` to the URL with `basePath` prepended, using `addPathPrefix`.
  - It copies mutable cookies into `set-cookie`.
- **Pages and server components, prerender and error path**: `app-render.tsx:9833-9846`, in `prerenderToStream` (function starts at line 8581). It applies the same status and `location` logic, without the cookie handling.
- **HTML body fallback**: `server/app-render/make-get-server-inserted-html.tsx:56-72` collects redirect errors seen during render. It injects `<meta id="__next-page-redirect" http-equiv="refresh" content="0;url=…">` for a permanent redirect (308) and `1;url=…` otherwise. This matters when the response has already started streaming, so the status can't change. The docs comment on `redirect()` says "insert a meta tag".
- **Server actions** (`server/app-render/action-handler.ts:1324-1360`):
  - For a fetch action, it keeps the status at 200 and sends the redirect through the `x-action-redirect` mechanism. It builds that with `createRedirectRenderResult`, and the body can include the target's Flight response. I did not read the header-setting code itself.
  - For a no-JS form post (MPA action), it sets status 303 and a `Location` header, with an empty body.

**Uncertainty:** I did not trace the header code inside `createRedirectRenderResult`. I also did not check how `app-render.tsx` behaves if headers have already been sent, beyond the meta-tag fallback above.