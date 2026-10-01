`redirect()` and `permanentRedirect()` never build a response. They throw a tagged error, and the server catches it and sets the status code and `Location` header. The exact path depends on where they were called. I read the page-render and route-handler paths. I did not read the server-action path (`action-handler.ts:1324`), so what happens there is not verified.

**1. Throwing the error** (`packages/next/src/client/components/redirect.ts`)
- `getRedirectError` (lines 9-17) creates `new Error(REDIRECT_ERROR_CODE)`. It sets `error.digest` to `` `${REDIRECT_ERROR_CODE};${type};${url};${statusCode};` ``.
- `redirect()` throws that error with `RedirectStatusCode.TemporaryRedirect`. Its type defaults to `'push'` inside a server action and `'replace'` otherwise (lines 34-41).
- `permanentRedirect()` throws it with `RedirectStatusCode.PermanentRedirect`. Its type defaults to `'replace'` (lines 55-60).
- The doc comments say 307 and 308 for route handlers. I did not open `redirect-status-code` to confirm the numeric values.
- The status code and URL are read back from the digest by `getRedirectStatusCodeFromError` and `getURLFromRedirectError` (lines 71-92). Both rely on `isRedirectError` from `redirect-error`.

**2. Catching it on the server, page and layout render** (`packages/next/src/server/app-render/app-render.tsx`)
- The catch blocks check `isRedirectError(err)` at about lines 4386 and 9833.
- They set `res.statusCode = getRedirectStatusCodeFromError(err)` and copy it to `metadata.statusCode`.
- They compute `redirectUrl = addPathPrefix(getURLFromRedirectError(err), basePath)` and call `setHeader('location', redirectUrl)` (about lines 9838-9845).
- Around lines 4393-4400, mutable cookies are also copied into a `set-cookie` header.
- The render then continues down the error-recovery path with `errorType = 'redirect'`. I only saw the start of that path, so I can't say what body is produced.
- The `redirect()` doc comment says that in a Server Component "this will insert a meta tag". I did not check where that tag is emitted. `make-get-server-inserted-html.tsx:56` also handles `isRedirectError`, and I believe that is where the tag is emitted, but I did not open it.

**3. Route handlers** (`packages/next/src/server/route-modules/app-route/module.ts`, `handleHandlerError`, line 424 onward)
- If `isRedirectError(err)` is true, it gets the URL from the error and builds `new Headers({ Location: url })`.
- It appends the mutable cookies and then returns `new Response(null, { status: ... })`.
- A comment says that in an action the status is 303, so the POST is not re-submitted.
- The outputs I read were cut off before the rest of that `status` expression. I did not see the non-action branch, which presumably uses the status code from the error.

**Not verified:** the server-action handling in `action-handler.ts`, and the rest of the error-recovery path after the redirect headers are set.