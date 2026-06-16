# Benchmark Gap Fixes

**Source:** [GitHub Benchmark — octocode vs gh](https://github.com/bgauryy/octocode-mcp/blob/main/benchmark/README.md)
**Status:** Proposed  
**Priority areas:** Q7 (oversized files), Q13 (inline PR comments), Q17 (org enumeration)

Three capability gaps where `gh` CLI scored higher than octocode in the benchmark run.
Each section documents: the root cause, how `gh` handles it, the current octocode state, and the exact fix required.

---

## Gap 1 — Oversized file access (Q7, CONTENT)

### What happened

> **Task:** Read `src/compiler/checker.ts` in `microsoft/TypeScript`. What does the last function do?
>
> - **gh score: 3** — read the entire ~3 MB file via the Git Blob API, identified `createBasicNodeBuilderModuleSpecifierResolutionHost` at line 54,260.
> - **octocode score: 1** — reported HTTP 413, answered `UNKNOWN` for the last function.

### How gh CLI does it

```bash
# gh fetches the repo tree, extracts the blob SHA, then fetches the full blob
gh api repos/microsoft/TypeScript/contents/src/compiler/checker.ts
# → 413 or empty content field; gh then falls back to:
gh api repos/microsoft/TypeScript/git/blobs/{sha}
# → returns full base64-encoded content, no inline size cap (up to 100 MB)
```

The Git Blob endpoint (`GET /repos/{owner}/{repo}/git/blobs/{file_sha}`) returns content even when the Contents API refuses. The tradeoff: the response is always fully base64-encoded (no streaming), so for a 3 MB file this means ~4 MB of base64 on the wire.

### Current octocode state

`fileContentRaw.ts` already has a blob fallback — **but it only triggers for files where the Contents API returns HTTP 200 with an empty `content` field and a present `sha`** (the 1 MB – 100 MB "silent fallback" path). For files where GitHub returns HTTP 413 directly, the error is caught by the outer handler and returned as an error response:

```typescript
// fileContentRaw.ts — current code (simplified)
try {
    result = await octokit.rest.repos.getContent(contentParams);
} catch (error: unknown) {
    if (error instanceof RequestError && error.status === 404) {
        // ... 404 handled
    } else {
        throw error;  // ← 413 falls here → handleGitHubAPIError → error response
    }
}
// ... later, for 200 + empty content:
} else if (fileSize > 0 && 'sha' in data && data.sha) {
    decoded = await fetchContentViaBlob(octokit, owner, repo, data.sha, filePath);
}
```

The `fetchContentViaBlob` helper already exists in the file and works correctly. The problem is it never gets called on a 413.

### Fix

**File:** `packages/octocode-mcp/src/github/fileContentRaw.ts`

Add a 413 handler that:
1. Fetches the parent directory listing to extract the target file's blob SHA.
2. Calls the existing `fetchContentViaBlob` with that SHA.

```typescript
// In fetchRawGitHubFileContent, extend the catch block:
} catch (error: unknown) {
    if (error instanceof RequestError && error.status === 404) {
        // ... existing 404 handling (unchanged)
    } else if (error instanceof RequestError && error.status === 413) {
        // File too large for the Contents API inline path.
        // Retrieve the blob SHA from the parent directory listing, then
        // fall back to the Git Blob API which has no inline size cap.
        return await fetchContentViaTreeFallback(
            octokit, owner, repo, filePath, branch || actualBranch, authInfo
        );
    } else {
        throw error;
    }
}
```

New helper to add to the same file:

```typescript
/**
 * Fallback for files that the Contents API rejects with HTTP 413.
 * 1. Fetches the parent directory listing to obtain the target file's blob SHA.
 * 2. Delegates to fetchContentViaBlob() which calls GET /git/blobs/{sha}.
 *
 * Supports files up to 100 MB (GitHub's Git Blob API limit).
 */
async function fetchContentViaTreeFallback(
    octokit: InstanceType<typeof OctokitWithThrottling>,
    owner: string,
    repo: string,
    filePath: string,
    branch?: string,
    authInfo?: AuthInfo
): Promise<GitHubAPIResponse<RawContentResult>> {
    try {
        const parentPath = filePath.split('/').slice(0, -1).join('/');
        const fileName = filePath.split('/').pop();
        if (!fileName) {
            return { error: 'Could not determine file name from path', type: 'unknown', status: 400 };
        }

        const ref = branch || (await resolveDefaultBranch(owner, repo, authInfo));

        // A directory listing (array response) includes each entry's blob SHA.
        const dirResult = await octokit.rest.repos.getContent({
            owner,
            repo,
            path: parentPath || '',
            ref,
        });

        if (!Array.isArray(dirResult.data)) {
            return { error: `Expected directory listing for ${parentPath}`, type: 'unknown', status: 500 };
        }

        const entry = (dirResult.data as Array<{ name: string; sha: string; type: string }>)
            .find(e => e.name === fileName && e.type === 'file');

        if (!entry) {
            return { error: `File ${fileName} not found in ${parentPath}`, type: 'unknown', status: 404 };
        }

        const decoded = await fetchContentViaBlob(octokit, owner, repo, entry.sha, filePath);
        if ('error' in decoded) return decoded as GitHubAPIResponse<RawContentResult>;

        return {
            data: {
                rawContent: decoded.data,
                branch: ref || undefined,
                resolvedRef: ref || 'HEAD',
            },
            status: 200,
        };
    } catch (err: unknown) {
        return handleGitHubAPIError(err);
    }
}
```

**Edge cases to handle:**
- Files > 100 MB: `git.getBlob()` returns `truncated: true`; the existing `decodeBase64Content` would decode the partial content silently. Add a check for `blobResult.data.truncated` and surface a warning in the response hint.
- Root-level files: `parentPath` is `''` (empty string), which is a valid path for `repos.getContent()`.
- Files that the researcher requested with a `charOffset`/`matchString`: the blob returns the full file; existing `processFileContentAPI` and `applyContentPagination` pipeline handles this correctly after raw content is available.

---

## Gap 2 — Inline PR review comments (Q13, PR)

### What happened

> **Task:** For PR #27733 in `facebook/react` — how many **inline review comments** are there? Quote the most substantive objection.
>
> - **gh score: 2** — correctly found 2 inline review comments via `GET /pulls/{pull}/comments`, quoted a comment.
> - **octocode score: 1** — reported 29 "total comments" (mixed inline + discussion), could not identify or quote a specific inline reviewer objection.

### How gh CLI does it

```bash
# gh explicitly hits the separate review-comments endpoint
gh api repos/facebook/react/pulls/27733/comments
# → returns only inline thread comments (code annotations), not PR-level discussion
# GET /repos/{owner}/{repo}/pulls/{pull_number}/comments

# vs. the issue/discussion comments endpoint:
gh api repos/facebook/react/issues/27733/comments
# → returns only PR-level discussion comments
```

GitHub exposes two distinct endpoints:

| Endpoint | What it returns |
|---|---|
| `GET /repos/{owner}/{repo}/pulls/{pull_number}/comments` | Inline code review thread comments |
| `GET /repos/{owner}/{repo}/issues/{issue_number}/comments` | PR-level discussion comments |
| `GET /repos/{owner}/{repo}/pulls/{pull_number}/reviews` | Review summaries (APPROVED, CHANGES_REQUESTED, with optional body) |

### Current octocode state

`prContentFetcher.ts` already implements BOTH `fetchPRComments` (discussion) and `fetchPRInlineComments` (inline). Both are called in parallel when `withComments: true`. The results are correctly tagged: discussion comments get `commentType: 'discussion'`; inline comments get `commentType: 'review_inline'`.

**The problem is in the output shape.** `prTransformation.ts`'s `formatPRForResponse` merges all comments into a flat `comment_details` array and reports only a single aggregate `comments` count:

```typescript
// prTransformation.ts — current (simplified)
return {
    ...
    comments: comments.length,              // ← mixed total, no breakdown
    comment_details: commentDetails,         // ← flat array, commentType buried in each item
    comment_details_shown: commentDetails.length,
    comment_details_total: comments.length,
};
```

The researcher saw `comments: 29` and interpreted it as "29 inline review comments" when the inline count was only 2. The `commentType` discriminator exists on each item but the summary-level count doesn't split it.

### Fix

**File:** `packages/octocode-mcp/src/github/prTransformation.ts`

Expose a structured comment summary that separates the two types:

```typescript
// In formatPRForResponse, replace the flat comments block with:

const inlineComments = comments.filter(c => c.commentType === 'review_inline');
const discussionComments = comments.filter(c => c.commentType !== 'review_inline');

return {
    ...
    // Replace single `comments: N` with a clear breakdown:
    comments_summary: {
        inline_review: inlineComments.length,   // ← code-level thread comments
        discussion: discussionComments.length,  // ← PR-level discussion comments
        total: comments.length,
    },
    // Inline comments first — they are the most research-relevant for code review:
    ...(inlineComments.length > 0 && {
        inline_review_comments: inlineComments.slice(0, options.includeFullCommentDetails ? undefined : SEARCH_RESULT_MAX_COMMENT_DETAILS).map(comment => ({
            ...comment,
            body: paginateText(comment.body, charOffset, commentCharLength, !options.includeFullCommentDetails).value ?? '',
        })),
    }),
    ...(discussionComments.length > 0 && {
        discussion_comments: discussionComments.slice(0, options.includeFullCommentDetails ? undefined : SEARCH_RESULT_MAX_COMMENT_DETAILS).map(comment => ({
            ...comment,
            body: paginateText(comment.body, charOffset, commentCharLength, !options.includeFullCommentDetails).value ?? '',
        })),
    }),
};
```

Also update the hint text in `prTransformation.ts` to name the types:

```typescript
// Replace the generic pagination hint with a type-aware one:
`${inlineComments.length} inline review comment(s) and ${discussionComments.length} discussion comment(s). Use prNumber with withComments=true to retrieve all.`
```

**Schema update required:**

Update `PRCommentItem` output schema in `tools/github_search_pull_requests/types.ts` and the JSON schema helpers in `scheme/fields.ts` to document the new `comments_summary` shape.

**Backward compatibility note:** The old `comment_details` / `comment_details_total` fields can be kept for one release with a deprecation note, or removed immediately since the schema is additive. The new split format is strictly more informative.

---

## Gap 3 — Org-level repo enumeration (Q17, REPOS)

### What happened

> **Task:** List every public repo in the `vercel` org. Total count? Top 5 by stars? How many have 1,000+ stars?
>
> - **gh score: 3** — reported 233 total repos, correct top-5, 44 repos with 1,000+ stars.
> - **octocode score: 1** — reported `UNKNOWN` for total and for 1,000+ count; top-5 was correct but arrived at via search, not enumeration.

### How gh CLI does it

```bash
# gh uses the Org Repos REST endpoint with automatic pagination
gh api orgs/vercel/repos \
    --paginate \
    --jq '[.[] | {name, stargazers_count, description}]'
# → GET /orgs/{org}/repos?per_page=100&page=N (repeated until exhausted)
# → returns ALL repos, no search cap
```

The endpoint `GET /orgs/{org}/repos` (also available for users as `GET /users/{username}/repos`):
- Is a **listing** endpoint, not a **search** endpoint — no 1,000-result cap
- Supports `type=public` filter, `sort=stars`, and full pagination
- Returns exact total via the `Link` header's `last` page rel
- Does NOT count against the search rate limit

### Current octocode state

`ghSearchRepos` exclusively uses `octokit.rest.search.repos()` (GitHub Search API), which:
- Is capped at **1,000 total results** per query
- Does not guarantee complete enumeration
- Does not expose the true total count of an org's repos

`repoSearch.ts` has no path to `octokit.rest.repos.listForOrg()` or `octokit.rest.repos.listForUser()`.

### Fix

**Approach:** Add an `orgRepos` parameter to `ghSearchRepos`. When `owner` is provided but no `keywordsToSearch` / `topicsToSearch`, route to the listing endpoint instead of search.

**Files to change:**

#### A. `packages/octocode-mcp/src/github/repoSearch.ts`

Add a new internal function `listGitHubOrgReposAPI`:

```typescript
/**
 * Lists ALL public repositories for an org or user using the REST listing
 * endpoint (GET /orgs/{org}/repos). Unlike search.repos(), this endpoint:
 *   - Has no 1,000-result cap
 *   - Can fully enumerate an org's repos
 *   - Supports sort=stars, type=public
 *   - Paginates automatically up to maxPages
 */
export async function listGitHubOrgReposAPI(
    params: { owner: string; sort?: 'stars' | 'updated' | 'created'; limit?: number; page?: number },
    authInfo?: AuthInfo
): Promise<GitHubAPIResponse<RepoSearchAPIData>> {
    try {
        const octokit = await getOctokit(authInfo);
        const perPage = Math.min(params.limit || 100, 100);
        const currentPage = params.page || 1;

        // Try org endpoint first; fall back to user endpoint on 404
        let result;
        try {
            result = await octokit.rest.repos.listForOrg({
                org: params.owner,
                type: 'public',
                sort: params.sort || 'stars',
                direction: 'desc',
                per_page: perPage,
                page: currentPage,
            });
        } catch (err: unknown) {
            if (err instanceof RequestError && err.status === 404) {
                // owner is a user, not an org
                result = await octokit.rest.repos.listForUser({
                    username: params.owner,
                    type: 'public',
                    sort: params.sort || 'stars',
                    direction: 'desc',
                    per_page: perPage,
                    page: currentPage,
                });
            } else {
                throw err;
            }
        }

        // Parse total count from Link header (rel="last" gives total pages)
        const totalPages = parseTotalPagesFromLinkHeader(result.headers.link);
        const estimatedTotal = totalPages ? (totalPages - 1) * perPage + result.data.length : null;
        const hasMore = result.data.length === perPage;

        const repositories = result.data.map(repo => ({
            owner: params.owner,
            repo: repo.name,
            defaultBranch: repo.default_branch,
            stars: repo.stargazers_count || 0,
            description: repo.description
                ? repo.description.substring(0, 150) + (repo.description.length > 150 ? '...' : '')
                : 'No description',
            url: repo.html_url,
            createdAt: repo.created_at,
            updatedAt: repo.updated_at,
            pushedAt: repo.pushed_at,
            visibility: repo.visibility,
            ...(repo.topics?.length && { topics: repo.topics }),
            ...(repo.forks_count && { forksCount: repo.forks_count }),
            ...(repo.language && { language: repo.language }),
        }));

        return {
            data: {
                repositories: repositories as GitHubRepositoryOutput[],
                ...(estimatedTotal !== null && { estimatedTotal }),
                pagination: {
                    currentPage,
                    totalPages: totalPages || (hasMore ? currentPage + 1 : currentPage),
                    perPage,
                    totalMatches: estimatedTotal ?? repositories.length,
                    hasMore,
                },
            },
            status: 200,
            rawResponseChars: countSerializedChars(result.data),
        };
    } catch (error: unknown) {
        return handleGitHubAPIError(error);
    }
}

/**
 * Parses the total page count from the GitHub Link response header.
 * Returns null if the header is absent or malformed.
 */
function parseTotalPagesFromLinkHeader(linkHeader?: string | null): number | null {
    if (!linkHeader) return null;
    const match = linkHeader.match(/[?&]page=(\d+)[^>]*>;\s*rel="last"/);
    return match ? parseInt(match[1], 10) : null;
}
```

#### B. `packages/octocode-mcp/src/github/repoSearch.ts` — routing

In `searchGitHubReposAPIInternal`, add the org-listing branch:

```typescript
async function searchGitHubReposAPIInternal(params, authInfo) {
    // New: if only owner is set (no search terms), use the listing endpoint
    const hasSearchTerms = (params.keywordsToSearch?.length ?? 0) > 0
        || (params.topicsToSearch?.length ?? 0) > 0;

    if (!hasSearchTerms && params.owner && !Array.isArray(params.owner)) {
        return await listGitHubOrgReposAPI(
            { owner: params.owner, sort: params.sort as 'stars' | 'updated', limit: params.limit, page: params.page },
            authInfo
        );
    }

    // ... existing search.repos() path unchanged
}
```

#### C. Tool schema — `ghSearchRepos`

Update the schema to document the new `owner`-only listing mode:

```
owner (optional):
  - Combined with keywords/topics: restricts search to that org/user scope.
  - Used ALONE (no keywords/topics): lists ALL public repos for the org/user
    via the REST listing endpoint (no 1,000-result cap). Returns exact total
    count and full pagination. Use sort=stars to rank by popularity.
```

#### D. Hints update — `tools/github_search_repositories/hints.ts`

Add a hint for org enumeration use case:

```
To count or enumerate all repos in an org: use owner alone without keywords.
Example: { "owner": "vercel" } → lists all repos with exact total count.
```

### Pagination strategy for large orgs

For orgs with hundreds of repos (e.g., `vercel` has 233), a single page of 100 returns the top 100 by stars. For the full count, the `estimatedTotal` field (derived from the `Link` header) gives an exact total without needing to page through everything. The agent can then make additional paginated calls if needed.

---

## Summary table

| Gap | Root cause | Fix location | Effort |
|---|---|---|---|
| **Q7** Oversized files | HTTP 413 not caught; no fallback to blob API | `fileContentRaw.ts` — add 413 handler + `fetchContentViaTreeFallback()` | Small (~50 LOC) |
| **Q13** Inline PR comments | Mixed inline + discussion count with no breakdown in output | `prTransformation.ts` — split `comments_summary` + `inline_review_comments` / `discussion_comments` | Small (~30 LOC) |
| **Q17** Org enumeration | Tool uses search API (1k cap); no listing endpoint path | `repoSearch.ts` — add `listGitHubOrgReposAPI()` + routing when `owner`-only | Medium (~80 LOC) |

All three fixes are **additive and backward-compatible**. No existing tool inputs change; new capability surfaces through existing parameters (413 error path, `withComments` response shape, `owner`-only query routing).

---

## References

- [GitHub REST — Git Blob API](https://docs.github.com/en/rest/git/blobs)
- [GitHub REST — Pull Request Review Comments](https://docs.github.com/en/rest/pulls/comments)
- [GitHub REST — List Org Repos](https://docs.github.com/en/rest/repos/repos#list-organization-repositories)
- [GitHub Contents API — large file behavior](https://docs.github.com/en/rest/repos/contents#size-limits)
- Benchmark run: `benchmark/github/output/summary.md`
