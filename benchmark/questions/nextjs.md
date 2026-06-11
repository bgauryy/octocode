# Questions

20 research questions about the `vercel/next.js` repository.

Clone the repo before answering local questions:
```bash
git clone --depth 1 https://github.com/vercel/next.js /tmp/nextjs-bench
```

---

### Q1 — Exhaustive `"use client"` directive coverage `[SEARCH]`

In `vercel/next.js`, find every file under `packages/next/src/` that contains `'use client'` as an actual directive (the first statement in the file), excluding the `compiled/` subdirectory.
1. How many unique files contain this directive?
2. List all file paths.

> *Tests exhaustive search result completeness across a large result set. Judge verifies the total count independently and checks whether any agent disclosed a pagination or result limit.*

---

### Q2 — Bulk symbol definition lookup `[SEARCH]`

Find where each of these symbols is **defined** (class or function declaration, not an import) in `vercel/next.js`:
- `NextRequest` — exact file path and line number
- `NextResponse` — exact file path and line number
- `ImageResponse` — exact file path and line number

> *Tests multi-target symbol lookup efficiency. D=3 requires exact file:line for all three, not just file paths.*

---

### Q3 — textMatch context: `revalidatePath` call sites `[SEARCH]`

In `vercel/next.js`, find every call to `revalidatePath` inside `packages/next/src/server/`.
For each match: state the file path, line number, and the exact source line.

> *Tests match-context quality alongside search results. Judge verifies that line numbers are correct and that the returned source line matches the actual file. D=3 requires exact source lines for every match.*

---

### Q4 — AND-intersection: files referencing both routers `[SEARCH]`

In `vercel/next.js`, find files under `packages/next/src/` that contain **both** `appDir` and `pagesDir` in the same file.
1. How many files match?
2. List all file paths.

> *Tests file-level AND-intersection query semantics. An OR-union result will include files containing only one term and overcount. Judge verifies the correct count independently.*

---

### Q5 — Large file targeted read: app-render entry `[CONTENT]`

Read `packages/next/src/server/app-render/app-render.tsx` in `vercel/next.js`.
1. What are the names of all top-level exported functions or types?
2. What does `renderToHTMLOrFlight` return and what are its required parameters?

> *Tests targeted reads on a multi-thousand-line file. D=3 requires the exact return type and parameter names, not a paraphrase.*

---

### Q6 — Directory listing with extension breakdown `[STRUCTURE]`

List all files directly inside `packages/next/src/server/` in `vercel/next.js` (non-recursive, flat listing only).
1. How many total files are in this directory?
2. How many have `.ts` vs `.tsx` extensions?

> *Tests flat directory listing with extension metadata. Judge verifies count and extension breakdown independently.*

---

### Q7 — Monorepo package enumeration `[STRUCTURE]`

In `vercel/next.js`, list every directory directly under `packages/`.
1. How many top-level packages exist?
2. List all package directory names.

> *Tests top-level tree navigation. D=3 requires a complete enumerated list — partial lists with "etc." score D≤1.*

---

### Q8 — Feature archaeology: Partial Prerendering `[PR]`

Search merged PRs in `vercel/next.js` to find the PR that first introduced Partial Prerendering (PPR).
1. What is the PR number and title?
2. What was the stated motivation in the PR body?
3. Which key files were added or changed?

> *Tests PR search combined with body and changed-file list access. D=3 requires PR number + body quote + file list.*

---

### Q9 — Inline review thread depth `[PR]`

In `vercel/next.js`, find a merged PR that introduced or significantly changed middleware execution in the App Router.
1. How many inline review comments (code-level thread comments, not PR-level summaries) does it have?
2. Which file received the most inline comments?
3. Quote the most substantive reviewer objection.

> *Inline thread comments are a separate resource from PR-level review summaries. An agent answering from summaries will miss the inline thread; the verbatim quote requirement exposes this. D=3 requires correct count + correct file + verbatim reviewer quote.*

---

### Q10 — npm registry: Next.js ecosystem packages `[PACKAGE]` `[drift]`

Look up the following npm packages. For each report: current version, weekly download count, and repository URL.
- `next`
- `create-next-app`
- `@next/bundle-analyzer`

What replaced the deprecated `@next/font` package, and where does it now live?

> *Tests registry lookup capability. D=3 requires version + weekly downloads + repo URL for all three packages, plus the font replacement answer.*

---

### Q11 — `"use server"` directive frequency `[LOCAL]`

Find every file under `packages/next/src/` that contains the exact string `"use server"`, excluding the `compiled/` subdirectory.
1. How many unique files contain it?
2. List all file paths.

> *Tests local exhaustive text search across a large codebase. Judge verifies the total count independently.*

---

### Q12 — TODO/FIXME annotation survey `[LOCAL]`

Find every `// TODO`, `// FIXME`, or `// HACK` comment in `packages/next/src/server/`.
For each: state the file path, line number, and exact comment text.
What is the total count?

> *Tests comment-text discovery. D=3 requires exact comment text, line number, and file path for every match. Judge verifies the total count.*

---

### Q13 — Largest source files by size `[LOCAL]`

Find the 5 largest `.ts` or `.tsx` files by byte size under `packages/next/src/server/`.
For each: state the file path and file size.

> *Tests file metadata queries. D=3 requires exact byte sizes for all 5 files in correct rank order.*

---

### Q14 — Server directory structure `[STRUCTURE]`

List all direct subdirectories under `packages/next/src/server/`.
1. How many subdirectories are there?
2. What does each appear to be responsible for based on its name?

> *Tests local directory tree navigation. D=3 requires a complete enumerated list with reasonable purpose descriptions for each subdirectory.*

---

### Q15 — Abstract class interface: `BaseServer` `[CONTENT]`

Read `packages/next/src/server/base-server.ts`.
1. What is the stated purpose of the `BaseServer` class?
2. How many `abstract` methods does it declare? List their names and return type signatures.

> *The stated class purpose may live in a doc comment or class header comment. D=3 requires the stated class purpose and the exact list of abstract method names + return types.*

---

### Q16 — Configuration defaults `[CONTENT]`

Read `packages/next/src/server/config-shared.ts`.
1. What is the default value for `reactStrictMode`?
2. What is the default value for `poweredByHeader`?
3. List 5 other top-level config keys with their default values.

> *Tests targeted local file content extraction. D=3 requires exact boolean/value defaults for all requested keys, verified against the source file.*

---

### Q17 — Symbol definition: `NextRequest` `[LSP]`

Where is the `NextRequest` class defined (not re-exported)?
1. State the exact file path and line number of the class declaration.
2. What does `NextRequest` extend?

> *The answer is the canonical declaration, not a re-export or wrapper. D=3 requires the exact file:line of the `class NextRequest` declaration and the correct extends target. Judge verifies via direct source inspection.*

---

### Q18 — Type definition: `Metadata` `[LSP]`

The main `Metadata` type lives under `packages/next/src/lib/metadata/types/`. Which file defines the root `Metadata` type or interface?
1. State the exact file name and line number of the type declaration.
2. List 5 of its optional fields with their value types.

> *Multiple files may export `Metadata`; only one defines the root type. D=3 requires the correct file name, the exact line number of the declaration, and 5 correct field names with TypeScript value types.*

---

### Q19 — Reference exhaustiveness: `unstable_cache` `[LSP]`

Find all files across `packages/` that reference or import `unstable_cache`, excluding the `compiled/` subdirectory.
1. How many unique files reference it?
2. List all file paths grouped by package.

> *Tests exhaustive cross-package search. D=3 requires a complete list with files grouped by package directory. Judge verifies the count independently.*

---

### Q20 — Incoming call hierarchy: `renderToHTMLOrFlight` `[LSP]`

Trace the direct callers of the function `renderToHTMLOrFlight` in the server rendering pipeline.
1. List each direct caller: function name, file path, and line number of the call site.
2. How many direct callers does it have?

> *Tests call-site discovery. D=3 requires every direct call site with function name, file:line. Judge verifies via direct source inspection.*
