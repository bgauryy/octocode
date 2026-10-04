**Short answer:** `redirect()` and `permanentRedirect()` don't build a response. They throw a special error whose `digest` string holds the URL and status code. Each server entry point catches that error and turns it into a status code and a `Location` header. There are three catch sites: page rendering, route handlers, and server actions.

**1. Throwing (`packages/next/src/client/components/redirect.ts`)**
- `getRedirectError` (lines 10–18) creates `new Error(REDIRECT_ERROR_CODE)`. It sets `digest = "${REDIRECT_ERROR_CODE};${type};${url};${statusCode};"`.
- `redirect()` (33–41) throws that error with `RedirectStatusCode.TemporaryRedirect`. The type defaults to `push` inside a server action and `replace` otherwise.
- `permanentRedirect()` (55–61) throws it with `RedirectStatusCode.PermanentRedirect`. The type defaults to `replace`.
- Helpers decode the digest. `getURLFromRedirectError` (71–77) returns the URL, `getRedirectTypeFromError` (79–85) the type, and `getRedirectStatusCodeFromError` (87–93) the status code. All of them check `isRedirectError`, which is imported from `redirect-error`. I didn't read that file.

**2. Pages (Server Components), in `packages/next/src/server/app-render/app-render.tsx`**
- Two catch paths look the same. One is at lines 4386–4403 and the other at 9839–9846. Each does the following:
  - It checks `isRedirectError(err)` and sets `errorType = 'redirect'`.
  - It sets `res.statusCode = getRedirectStatusCodeFromError(err)`, so the status is 307 for `redirect()` and 308 for `permanentRedirect()`.
  - It builds the URL with `addPathPrefix(getURLFromRedirectError(err), basePath)`.
  - It calls `setHeader('location', redirectUrl)`.
- The first path also copies cookies into `set-cookie` (around line 4400) before it sets `location`.
- I didn't read the code between lines 4391 and 4400, or the surrounding function bodies, so I don't know exactly when each path runs.
- Both paths rely on the response not having started streaming. I didn't check what happens if the throw comes after the shell has been flushed. The doc comment in `redirect.ts` says a Server Component redirect "will insert a meta tag". I didn't look at where that tag is inserted.

**3. Route handlers (`packages/next/src/server/route-modules/app-route/module.ts`, `handleHandlerError`, lines 417–450)**
- It checks `isRedirectError(err)` and gets the URL from `getURLFromRedirectError`. It throws an invariant error if the URL is empty.
- It builds `new Headers({ Location: url })` and appends cookies with `appendMutableCookies`.
- It returns `new Response(null, { status, headers })`. The status is `303` (`SeeOther`) if `actionStore.isAction`. Otherwise it is `getRedirectStatusCodeFromError(err)`, which is 307 or 308.

**4. Server actions (`packages/next/src/server/app-render/action-handler.ts`)**
- The catch at lines 1324–1326 uses `isRedirectError`, `getURLFromRedirectError` and `getRedirectTypeFromError`.
- A helper taking `redirectType` is declared at line 388. I didn't read its body, so I can't say exactly how it builds the response.
- The doc comment in `redirect.ts` says a redirect in a server action becomes a client-side navigation when JS is available, and a 303 for a progressive-enhancement form post. I confirmed the 303 only in the route-handler code above.