# Next.js — 20 Research Questions

20 research questions about `vercel/next.js`. Answer using whatever tooling you have available.

---

## Section 1 — Remote Only (Q1–Q10)

> **MUST:** Answer Q1–Q10 using the **remote repository only** . Do **not** clone the repo for this section.

---

### Q1 — How does `notFound()` propagate to the not-found boundary?

In `vercel/next.js`, trace how `notFound()` interrupts App Router rendering and triggers the nearest `not-found.tsx` segment.
1. Where is `notFound()` defined? State the exact file path and line number.
2. What does it throw? Quote the relevant source line.
3. Where is that thrown value caught and converted into `not-found` segment rendering? State the file path and function name.

> *Tests multi-file code tracing. D=3 requires exact file:line for the definition, a verbatim quote of the throw mechanism, and the correct catch site. Agents that stop at the public re-export without finding the internal throw + catch chain score D≤1.*

---

### Q2 — Bulk symbol definition lookup

Find where each of these symbols is **defined** (class or function declaration, not an import) in `vercel/next.js`:
- `NextRequest` — exact file path and line number
- `NextResponse` — exact file path and line number
- `ImageResponse` — exact file path and line number

> *Tests multi-target lookup. D=3 requires exact file:line for all three — not just filenames. Agents that return import sites instead of declaration sites score D≤1.*

---

### Q3 — `revalidatePath` call sites

In `vercel/next.js`, find every call to `revalidatePath` inside `packages/next/src/server/`.
For each match: state the file path, line number, and the exact source line.

> *Tests search completeness and match-context quality. Judge verifies that line numbers are correct and that returned source lines match the actual file. D=3 requires exact source lines for every match, not paraphrases. Judge independently verifies the total count.*

---

### Q4 — Files referencing both routers

In `vercel/next.js`, find files under `packages/next/src/` that contain **both** `appDir` and `pagesDir` in the same file.
1. How many files match?
2. List all file paths.

> *Tests file-level AND-intersection. A result set that includes files containing only one of the two terms will overcount. Judge verifies the correct count independently.*

---

### Q5 — How does `redirect()` work in Server Components?

In `vercel/next.js`, trace how the `redirect()` function works end-to-end in the App Router — from the call site in a Server Component through to the HTTP response.
1. Where is `redirect()` defined? State the exact file path and line number.
2. What mechanism does it use to interrupt rendering? Quote the relevant source line(s).
3. Where is that signal caught and converted into an HTTP redirect response? State the file path and function name.

> *Tests multi-hop code tracing. D=3 requires exact file:line for the definition, a verbatim quote of the interruption mechanism, and the correct catch/response site. Agents that stop at the public re-export without tracing the internal throw + catch chain score D≤1.*

---

### Q6 — `renderToHTMLOrFlight` signature

Read `packages/next/src/server/app-render/app-render.tsx` in `vercel/next.js`.
1. What does `renderToHTMLOrFlight` return? State the exact return type.
2. List its parameters by name and type.
3. What is the first thing the function does before any rendering work? Quote the relevant line.

> *Tests targeted reads on a multi-thousand-line file. D=3 requires the exact return type, all parameter names + types, and a verbatim quote of the opening logic — not a summary.*

---

### Q7 — How does `revalidateTag` invalidate cached data?

In `vercel/next.js`, trace how calling `revalidateTag(tag)` invalidates cached entries end-to-end.
1. Where is `revalidateTag` defined on the server side? State the exact file path and line number.
2. What data structure does it write to when called? Quote the relevant source line(s).
3. Where does the server read that structure to decide what to revalidate? State the file path and function name.

> *Tests multi-hop architecture tracing through the cache invalidation pipeline. D=3 requires file:line for the definition, a verbatim quote of the write operation, and the correct consumer location. Agents that find only the public export without tracing the cache store write + consumer read score D≤1.*

---

### Q8 — How does a Server Action request reach the server?

In `vercel/next.js`, trace how an HTTP request carrying a Server Action is identified and routed on the server.
1. What HTTP header does Next.js use to identify a Server Action request? State the file path and the exact header name or constant.
2. Which function handles the Server Action execution? State its file path and name.
3. How does Next.js return the action result to the client? Quote the relevant response-building line.

> *Tests cross-layer architecture tracing from HTTP edge to server execution. D=3 requires the correct header/constant name with file:line, the correct executor function with location, and a verbatim response-building quote.*

---

### Q9 — Partial Prerendering introduction

Search merged PRs in `vercel/next.js` for the PR that introduced **Partial Prerendering (PPR)**.
1. State the PR number and exact title.
2. Quote the paragraph from the PR body that describes the motivation or user-facing goal.
3. List the key files added or modified to implement it.

> *Tests PR search combined with body access and changed-file enumeration. D=3 requires PR number + verbatim body quote + file list. Judge independently verifies all three against the cited PR. Agents that return only the title without accessing the PR body score D≤1.*

---

### Q10 — Inline review thread: Server Actions introduction

Search merged PRs in `vercel/next.js` for the PR that introduced Server Actions as a feature.
1. State the PR number and title.
2. How many inline review comments (code-level thread comments, not top-level PR review summaries) does it have?
3. Quote the most substantive reviewer objection from an inline comment thread.

> *Inline thread comments are a separate resource from PR-level review summaries. Agents that answer from summary data alone will miss or fabricate the inline threads. D=3 requires correct PR number + correct inline comment count + verbatim inline quote. Judge verifies all three against the cited PR.*

---

## Section 2 — Local Clone Required (Q11–Q20)

> **MUST:** Clone the repo before answering Q11–Q20. Do **not** answer this section without a local clone.
> ```bash
> git clone --depth 1 https://github.com/vercel/next.js /tmp/nextjs-bench
> ```

---

### Q11 — How does RSC streaming connect to the HTTP response?

In the local clone, trace how the React Server Component (RSC) stream is piped into the HTTP response in the App Router.
1. Find the file in `packages/next/src/server/` that initiates the RSC-to-HTTP stream. State the file path and the function name that starts streaming.
2. What API is used to connect the RSC output to the HTTP response? Quote the relevant line.
3. How does the server signal the end of the stream? Find and quote the relevant line.

> *Tests local architecture tracing across the rendering → network boundary. D=3 requires the correct file + function name, a verbatim stream-connection quote, and the stream-end signal.*

---

### Q12 — TODO/FIXME annotation survey

Find every `// TODO`, `// FIXME`, or `// HACK` comment in `packages/next/src/server/`.
For each match: state the file path, line number, and exact comment text.
What is the total count?

> *Tests exhaustive local search completeness. D=3 requires exact comment text, line number, and file path for every match. Judge verifies total count against a fresh clone snapshot. Note: these annotations change frequently — count and content will shift across repo versions.*

---

### Q13 — How does `generateStaticParams` connect to static build output?

In the local clone, trace how exported `generateStaticParams` functions in route files connect to the Next.js build's static HTML generation.
1. Find the file in `packages/next/src/build/` that calls `generateStaticParams` during the build. State the file path and the function name that invokes it.
2. What does that function do with the returned params array? Quote the key processing line.
3. How do the collected params feed into static page generation — what is the next function in the chain? State its name and file.

> *Tests cross-phase architecture tracing from route segment through the build pipeline to static output. D=3 requires the correct invoker file + function, a verbatim quote of the param processing, and the next-in-chain function with location.*

---

### Q14 — Server directory structure

List all direct subdirectories under `packages/next/src/server/`.
1. How many subdirectories are there?
2. List all their names.
3. Which subdirectory contains the most immediate children (files + subdirs combined)? State its name and child count.

> *Tests local directory tree navigation. D=3 requires a fully enumerated list and a correctly computed answer to sub-question 3. Sub-question 3 is independently verifiable.*

---

### Q15 — Abstract class interface: `BaseServer`

Read `packages/next/src/server/base-server.ts`.
1. What is the stated purpose of the `BaseServer` class? Quote the doc comment if present.
2. How many `abstract` methods does it declare? List every name and its return type signature.

> *Tests targeted reads on a large file. D=3 requires the stated class purpose (verbatim if there's a doc comment) and the complete list of abstract method names + return types — not a subset.*

---

### Q16 — Configuration defaults

Read `packages/next/src/server/config-shared.ts`.
1. What is the default value for `reactStrictMode`?
2. What is the default value for `poweredByHeader`?
3. List 5 other top-level config keys with their default values.

> *Tests targeted extraction from a large configuration object. D=3 requires exact boolean/value defaults for all requested keys, verified against the source file.*

---

### Q17 — Full class inheritance chain: `NextNodeServer`

Trace the complete class inheritance chain from `NextNodeServer` up to the root base class.
1. List every class in the chain in order (leaf → root), with its file path and line number of the `class` declaration.
2. How many levels deep is the chain?
3. Do any classes in the chain declare TypeScript interfaces they `implement`? List them.

> *Tests multi-hop class hierarchy traversal. Each hop requires finding the parent class across a different file. D=3 requires the complete ordered chain with exact file:line for every class, and a correct list of any implemented interfaces. Judge verifies via direct source inspection.*

---

### Q18 — All concrete implementations of `sendRenderResult`

The abstract method `sendRenderResult` is declared in `BaseServer`. Find every concrete override of this method across the entire codebase.
1. List each implementing class with its file path and the line number of the override.
2. For each: does the implementation call `super.sendRenderResult()`? Answer for each class.
3. Are any of the overriding classes themselves abstract?

> *Tests precision in finding method overrides vs all occurrences (calls, declarations, and overrides appear in the same text search). D=3 requires the correct list of overrides only, correct `super` call answers for each, and correct abstract/concrete classification. Judge verifies via direct source inspection.*

---

### Q19 — Reference exhaustiveness: `unstable_cache`

Find all files across `packages/` that reference or import `unstable_cache`, excluding the `compiled/` subdirectory.
1. How many unique files reference it?
2. List all file paths grouped by package directory.

> *Tests exhaustive cross-package search completeness. D=3 requires a complete list with files correctly grouped by package directory. Judge verifies the total count independently. Partial or ungrouped results score D≤1.*

---

### Q20 — Incoming call hierarchy: `renderToHTMLOrFlight`

Trace the direct callers of the function `renderToHTMLOrFlight` in the server rendering pipeline.
1. List every direct caller: function name, file path, and line number of the call site.
2. How many direct callers does it have?

> *Tests call-site discovery. Text search returns all occurrences including the declaration, type annotations, and string references — the correct answer requires filtering to actual call sites only. D=3 requires every direct call site with function name + file:line. Judge verifies via direct source inspection.*
