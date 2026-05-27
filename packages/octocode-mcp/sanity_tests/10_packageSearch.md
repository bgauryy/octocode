# Sanity Test: `packageSearch`

---

## Tool Overview

Searches the npm package registry. Returns package metadata including name, description, version, repository URL, and publish date. **Environment note:** npm search uses `npm config get registry`. When that points to a private registry (e.g. corporate Artifactory), public packages like `express`/`zod` may return empty. Public registry works.

## Enhanced Testing Requirements

**ALL test cases must validate:**
1. **Queries Structure** - Every query includes `mainResearchGoal`, `researchGoal`, and `reasoning`
2. **Pagination/Limits** - Test `searchLimit` parameter; verify different limits return different result counts
3. **Response Validation** - Every Expected section includes explicit response checking
4. **Hints Validation** - **GOLDEN**: Check response hints for user guidance and next steps

### Response Validation Pattern
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with tool-specific fields (packages)
  - [ ] Status-specific hints present
  - [ ] Hints suggest githubViewRepoStructure, package exploration, or further analysis

---

## Test Cases

### TC-1: npm — Popular Package (express)

**Goal:** Verify npm search for well-known package.

**Note:** Fails when `npm config get registry` points to a private registry (no public packages). Passes on public registry.

```json
{
  "mainResearchGoal": "Find express package",
  "researchGoal": "npm search for express",
  "reasoning": "express is a top-5 npm package — must be findable",
  "name": "express",
  "ecosystem": "npm"
}
```

**Expected:**
- [ ] Returns express with repo URL, version, description (when using public registry)
- [ ] May return empty when npm config points to private registry (environment-dependent)

---

### TC-2: npm — Popular Package (zod) with Metadata

**Goal:** Verify npm metadata fetch.

**Note:** Same environment dependency as TC-1 (npm registry config).

```json
{
  "queries": [{
    "mainResearchGoal": "Find zod package with metadata",
    "researchGoal": "npm search with metadata",
    "reasoning": "zod is a popular validation library",
    "name": "zod",
    "ecosystem": "npm",
    "npmFetchMetadata": true
  }]
}
```

**Expected:**
- [ ] Returns zod with version, description, lastPublished (when using public registry)
- [ ] May return empty when npm config points to private registry (environment-dependent)

---

### TC-3: npm — Scoped Package (mcp)

**Goal:** Verify npm search works for scoped/private packages.

```json
{
  "queries": [{
    "mainResearchGoal": "Find MCP packages on npm",
    "researchGoal": "npm scoped package search",
    "reasoning": "Scoped packages (@wix) work while public doesn't",
    "name": "mcp",
    "ecosystem": "npm",
    "searchLimit": 5
  }]
}
```

**Expected:**
- [ ] Returns results (5 @wix scoped packages expected)
- [ ] No timeout (previously timed out at 30s)
- [ ] Package names, descriptions present

---

### TC-4: npm — Search Limit

**Goal:** Verify `searchLimit` controls number of results.

```json
{
  "mainResearchGoal": "Test search limit on npm",
  "researchGoal": "Verify searchLimit parameter",
  "reasoning": "searchLimit should cap results",
  "name": "react",
  "ecosystem": "npm",
  "searchLimit": 3
}
```

**Expected:**
- [ ] At most 3 results (if any — may be empty due to npm bug)
- [ ] Or empty result (known bug for public packages)
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with package metadata
  - [ ] Status-specific hints present
  - [ ] Hints suggest actionable next steps relevant to the query
- [ ] **Hints Validation:**
  - [ ] Hints suggest alternative search or githubSearchRepositories

---

### TC-5: npm — Unknown Package

**Goal:** Verify graceful handling of non-existent package.

```json
{
  "mainResearchGoal": "Test error handling",
  "researchGoal": "Search for non-existent package",
  "reasoning": "Tool should handle missing packages gracefully",
  "name": "this-package-does-not-exist-xyz-99999",
  "ecosystem": "npm"
}
```

**Expected:**
- [ ] No error thrown
- [ ] Empty or "not found" result
- [ ] Clear indication package doesn't exist
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] Status-specific hints present (empty/error hints)
  - [ ] Hints suggest package name verification, alternative search
- [ ] **Hints Validation:**
  - [ ] Hints suggest package name verification, alternative search strategies

---

### TC-6: npm — Specific Scoped Package

**Goal:** Verify npm finds a known scoped package directly.

```json
{
  "queries": [{
    "mainResearchGoal": "Find specific scoped npm package",
    "researchGoal": "Direct scoped package lookup",
    "reasoning": "Scoped packages work — verify direct lookup",
    "name": "@modelcontextprotocol/sdk",
    "ecosystem": "npm",
    "searchLimit": 1
  }]
}
```

**Expected:**
- [ ] Returns the specific SDK package
- [ ] Version, description, repo URL present
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with package metadata
  - [ ] Status-specific hints present
  - [ ] Hints suggest actionable next steps relevant to the query
- [ ] **Hints Validation:**
  - [ ] Hints suggest githubViewRepoStructure, package exploration, or further analysis

---

### TC-7: Search Limit Maximum (Boundary)

**Goal:** Verify `searchLimit: 10` (maximum) works correctly.

```json
{
  "queries": [{
    "mainResearchGoal": "Test searchLimit boundary",
    "researchGoal": "Maximum search limit",
    "reasoning": "Max limit should return many packages",
    "name": "react",
    "ecosystem": "npm",
    "searchLimit": 10
  }]
}
```

**Expected:**
- [ ] Up to 10 packages returned (or empty due to npm bug)
- [ ] No timeout or error at maximum value
- [ ] All entries valid
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with package metadata
  - [ ] Status-specific hints present
  - [ ] Hints suggest actionable next steps relevant to the query
- [ ] **Hints Validation:**
  - [ ] Hints suggest githubViewRepoStructure, package exploration, or further analysis

---

### TC-8: Search Limit Minimum (Boundary)

**Goal:** Verify `searchLimit: 1` (minimum/default) returns exactly one result.

```json
{
  "queries": [{
    "mainResearchGoal": "Test searchLimit boundary",
    "researchGoal": "Minimum search limit",
    "reasoning": "Default limit is 1, verify exact behavior",
    "name": "express",
    "ecosystem": "npm",
    "searchLimit": 1
  }]
}
```

**Expected:**
- [ ] Exactly 1 package returned
- [ ] The most relevant match for "express"
- [ ] Rich metadata present
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with package metadata
  - [ ] Status-specific hints present
  - [ ] Hints suggest actionable next steps relevant to the query
- [ ] **Hints Validation:**
  - [ ] Hints suggest githubViewRepoStructure, package exploration, or further analysis

---

### TC-9: Empty Name (Validation)

**Goal:** Verify empty `name` is rejected.

```json
{
  "queries": [{
    "mainResearchGoal": "Test validation",
    "researchGoal": "Empty name validation",
    "reasoning": "Name is required and must be at least 1 char",
    "name": "",
    "ecosystem": "npm"
  }]
}
```

**Expected:**
- [ ] Validation error about name being empty
- [ ] Clear error message
- [ ] No search executed
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] Status-specific hints present (validation error hints)
  - [ ] Hints suggest providing valid package name
- [ ] **Hints Validation:**
  - [ ] Hints suggest name is required, min 1 char

---

### TC-10: Name with Special Characters

**Goal:** Verify package names with special characters are handled.

```json
{
  "queries": [{
    "mainResearchGoal": "Test special characters",
    "researchGoal": "Package name with special chars",
    "reasoning": "Some packages have hyphens, underscores, dots",
    "name": "ts-node",
    "ecosystem": "npm",
    "searchLimit": 3
  }]
}
```

**Expected:**
- [ ] Search completes without error
- [ ] Relevant results for "ts-node" returned (or empty due to npm bug)
- [ ] Special characters handled correctly
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with package metadata
  - [ ] Status-specific hints present
  - [ ] Hints suggest actionable next steps relevant to the query
- [ ] **Hints Validation:**
  - [ ] Hints suggest githubViewRepoStructure, package exploration, or further analysis

---

### TC-11: npm — npmFetchMetadata with Scoped Package

**Goal:** Verify `npmFetchMetadata: true` works with scoped packages.

```json
{
  "queries": [{
    "mainResearchGoal": "Test metadata fetch on scoped package",
    "researchGoal": "Scoped npm metadata",
    "reasoning": "Scoped packages should support metadata fetch",
    "name": "@modelcontextprotocol/sdk",
    "ecosystem": "npm",
    "npmFetchMetadata": true,
    "searchLimit": 1
  }]
}
```

**Expected:**
- [ ] Package found with full metadata
- [ ] Version, description, lastPublished present
- [ ] Repository URL present
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with package metadata
  - [ ] Status-specific hints present
  - [ ] Hints suggest actionable next steps relevant to the query
- [ ] **Hints Validation:**
  - [ ] Hints suggest githubViewRepoStructure, package exploration, or further analysis

---

### TC-12: Bulk Queries

**Goal:** Verify bulk queries against the npm ecosystem.

```json
{
  "queries": [
    {"mainResearchGoal": "Bulk test", "researchGoal": "npm package", "reasoning": "Test", "name": "mcp", "ecosystem": "npm", "searchLimit": 3},
    {"mainResearchGoal": "Bulk test", "researchGoal": "npm package", "reasoning": "Test", "name": "lodash", "ecosystem": "npm"},
    {"mainResearchGoal": "Bulk test", "researchGoal": "Non-existent", "reasoning": "Test", "name": "nonexistent-pkg-xyz-99999", "ecosystem": "npm"}
  ]
}
```

**Expected:**
- [ ] First query returns npm results
- [ ] Second query returns npm result
- [ ] Third query returns empty/not-found
- [ ] Each result isolated per query
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with package metadata per query
  - [ ] Status-specific hints present for each result
  - [ ] Hints suggest actionable next steps per status
- [ ] **Hints Validation:**
  - [ ] Success hints for first two; empty/error hints for third

---

### TC-13: Pagination — searchLimit Returns Different Counts

**Goal:** Dedicated pagination test — verify `searchLimit` controls result count; searchLimit 1 vs 5 returns different counts when multiple matches exist.

**Step 1 — searchLimit 1:**
```json
{
  "queries": [{
    "mainResearchGoal": "Test searchLimit pagination",
    "researchGoal": "Get 1 result with searchLimit:1",
    "reasoning": "Establish baseline for searchLimit comparison",
    "name": "mcp",
    "ecosystem": "npm",
    "searchLimit": 1
  }]
}
```

**Step 2 — searchLimit 5:**
```json
{
  "queries": [{
    "mainResearchGoal": "Test searchLimit pagination",
    "researchGoal": "Get up to 5 results with searchLimit:5",
    "reasoning": "Higher limit should return more when available",
    "name": "mcp",
    "ecosystem": "npm",
    "searchLimit": 5
  }]
}
```

**Expected:**
- [ ] Step 1 returns at most 1 package
- [ ] Step 2 returns up to 5 packages (when available)
- [ ] Step 2 count >= Step 1 count when multiple matches exist
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with package metadata
  - [ ] Status-specific hints present
  - [ ] Hints suggest actionable next steps
- [ ] **Hints Validation:**
  - [ ] Hints suggest githubViewRepoStructure, package exploration, or further analysis

---

## Known Issues

| Issue | Status | Details |
|-------|--------|---------|
| npm public packages empty | **ENV-DEPENDENT** | When `npm config get registry` points to private registry, `express`/`zod` return empty. Public registry works. |
| npm timeout | **FIXED** | `mcp` search no longer times out |

---

## Schema Edge Cases & Boundary Tests

**Schema reference (input):** `queries` length **1–3**. Per query: `ecosystem` **`npm`** (defaults to `npm`); `searchLimit` **1–10**; `name` **minLength 1**; optional response shaping: **`charLength` 1–50000** where the tool exposes it. Each query includes **`mainResearchGoal`**, **`researchGoal`**, and **`reasoning`**.

### SE-1: Empty `queries` array

**Goal:** Reject a bulk payload with zero queries.

```json
{
  "queries": []
}
```

**Expected:**
- [ ] Validation or tool error — empty `queries` not allowed
- [ ] **Response Validation:** if bulk error shape applies, `instructions` / `results` reflect the failure
- [ ] No registry search executed

### SE-2: `queries` over maximum (4 entries, max 3)

**Goal:** Enforce at most **3** queries per request.

```json
{
  "queries": [
    {"mainResearchGoal": "A", "researchGoal": "q1", "reasoning": "edge", "name": "a", "ecosystem": "npm"},
    {"mainResearchGoal": "B", "researchGoal": "q2", "reasoning": "edge", "name": "b", "ecosystem": "npm"},
    {"mainResearchGoal": "C", "researchGoal": "q3", "reasoning": "edge", "name": "c", "ecosystem": "npm"},
    {"mainResearchGoal": "D", "researchGoal": "q4", "reasoning": "edge", "name": "d", "ecosystem": "npm"}
  ]
}
```

**Expected:**
- [ ] Validation error (too many queries)
- [ ] Message or hints reference maximum **3**

### SE-3: Invalid `ecosystem` (not `npm`)

**Goal:** Reject values outside `npm`.

```json
{
  "queries": [{
    "mainResearchGoal": "Test",
    "researchGoal": "invalid ecosystem",
    "reasoning": "Only npm allowed",
    "name": "lodash",
    "ecosystem": "rust"
  }]
}
```

**Expected:**
- [ ] Validation error on `ecosystem`
- [ ] No npm call with invalid ecosystem

### SE-4: `searchLimit` 0 and 11 (outside 1–10)

**Goal:** Enforce `searchLimit` **1–10**.

**Below minimum:**
```json
{
  "queries": [{
    "mainResearchGoal": "Test",
    "researchGoal": "searchLimit 0",
    "reasoning": "Below min",
    "name": "react",
    "ecosystem": "npm",
    "searchLimit": 0
  }]
}
```

**Above maximum:**
```json
{
  "queries": [{
    "mainResearchGoal": "Test",
    "researchGoal": "searchLimit 11",
    "reasoning": "Above max",
    "name": "react",
    "ecosystem": "npm",
    "searchLimit": 11
  }]
}
```

**Expected:**
- [ ] Both payloads fail validation (or equivalent explicit error)
- [ ] Error or hints mention allowed range **1–10**

### SE-5: Empty `name` (`minLength` 1)

**Goal:** Reject zero-length package name (schema edge; overlaps TC-9).

```json
{
  "queries": [{
    "mainResearchGoal": "Test",
    "researchGoal": "empty name",
    "reasoning": "minLength 1",
    "name": "",
    "ecosystem": "npm"
  }]
}
```

**Expected:**
- [ ] Validation error; no search executed
- [ ] **Response Validation:** per-query `error` / validation path matches bulk conventions

### SE-6: Response-level pagination / truncation

**Goal:** If the response is paginated, truncated, or chunked (`charLength` / continuation hints), behavior is consistent and documented.

**Expected:**
- [ ] **Response Validation:** `instructions` and any pagination metadata align with returned chunks
- [ ] When more data exists, hints or fields point to the next page/chunk
- [ ] If `charLength` applies, values stay within **1–50000**

### SE-7: Duplicate query `id` values

**Goal:** Duplicate `id` in one request is rejected or handled deterministically.

```json
{
  "queries": [
    {"id": "dup", "mainResearchGoal": "A", "researchGoal": "a", "reasoning": "edge", "name": "left-pad", "ecosystem": "npm"},
    {"id": "dup", "mainResearchGoal": "B", "researchGoal": "b", "reasoning": "edge", "name": "lodash", "ecosystem": "npm"}
  ]
}
```

**Expected:**
- [ ] Validation error **or** explicit dedup/merge rules (record observed behavior)
- [ ] **Response Validation:** `results[].id` matches accepted queries 1:1 when duplicates are rejected

### SE-8: Minimal query — only required fields

**Goal:** Smallest valid query; optional fields use defaults.

```json
{
  "queries": [{
    "mainResearchGoal": "Minimal",
    "researchGoal": "Required fields only",
    "reasoning": "Optional omitted",
    "name": "express",
    "ecosystem": "npm"
  }]
}
```

**Expected:**
- [ ] Request accepted; defaults for `searchLimit` and other optionals are sane
- [ ] **Response Validation:** standard bulk `instructions`, `results`, `data` for package search

---

## Validation Checklist

| # | Test Case | Status |
|---|-----------|--------|
| 1 | npm express (env-dependent) | |
| 2 | npm zod + metadata (env-dependent) | |
| 3 | npm scoped mcp | |
| 4 | npm search limit | |
| 5 | npm unknown package | |
| 6 | npm scoped package | |
| 7 | Search Limit Maximum (Boundary) | |
| 8 | Search Limit Minimum (Boundary) | |
| 9 | Empty Name (Validation) | |
| 10 | Name with Special Characters | |
| 11 | npm — npmFetchMetadata with Scoped Package | |
| 12 | Bulk Queries | |
| 13 | Pagination — searchLimit Returns Different Counts | |
