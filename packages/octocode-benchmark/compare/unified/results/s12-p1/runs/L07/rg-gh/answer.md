**Short answer:** `redirect()` and `permanentRedirect()` don't build a response. They throw a special error whose `digest` string encodes the target URL and status. Server code catches that error and sets `Location` and the status code. Which code catches it depends on the context: page render, route handler, or server action.

All paths below are under `packages/next/src/`.

**1. Throwing the error**
- `client/components/redirect.ts:7-15`: `getRedirectError` creates `new Error('NEXT_REDIRECT')`. It sets `digest = "NEXT_REDIRECT;<type>;<url>;<status>;"`.
- `redirect()` throws that error with status 307. Its type defaults to `push` inside a server action and `replace` otherwise (`redirect.ts:36-43`).
- `permanentRedirect()` throws it with status 308 and type `replace` (`redirect.ts:61-66`).
- `client/components/redirect-status-code.ts` defines the codes: 303, 307 and 308.
- `client/components/redirect-error.ts:16-40`: `isRedirectError` recognises the error by parsing the digest.
- `redirect.ts:77-102`: `getURLFromRedirectError` and `getRedirectStatusCodeFromError` read the URL and status back out of the digest.

**2. Catching the error and writing the HTTP response**
- **Page render (RSC and HTML):** `server/app-render/app-render.tsx:4386-4402`.
  - It sets `res.statusCode = getRedirectStatusCodeFromError(err)`.
  - It prefixes the URL with `basePath` and sets the `location` header.
  - It copies any mutable cookies into `set-cookie`.
  - A second, similar branch is at `app-render.tsx:9833-9846`. It sets the status and the `location` header, but I didn't see it handle cookies.
- **HTML fallback for redirects that happen after streaming has started:** `server/app-render/make-get-server-inserted-html.tsx:56-71`.
  - Once the status line has been sent, it can't change the status.
  - Instead it injects `<meta id="__next-page-redirect" http-equiv="refresh" content="0;url=…">`. The delay is 0 for permanent redirects and 1 for temporary ones.
  - This matches the doc comment on `redirect()`, which says a Server Component inserts a meta tag.
- **Route handlers:** `server/route-modules/app-route/module.ts:424-449`.
  - It returns `new Response(null, { status, headers: { Location } })`, with mutable cookies appended.
  - The status is 303 if the handler ran as a server action. Otherwise it comes from the digest (307 or 308).
- **Server actions:** `server/app-render/action-handler.ts:1324-1357`.
  - A fetch action (a client-side call) returns a 200 and sends the redirect through the `x-action-redirect` mechanism. The body is built by `createRedirectRenderResult`, so no HTTP redirect happens.
  - A progressive-enhancement (MPA) form action returns 303 with a `Location` header and an empty body.

**Uncertainty:** I didn't trace the `x-action-redirect` header-setting inside `createRedirectRenderResult`. I also didn't check what the two `app-render.tsx` branches are for, beyond the code shown. The first looks like the main render error path and the second like the prerender error path.