# Sanity Test: `githubCloneRepo`

---

## Tool Overview

Clones a GitHub repository (shallow, `--depth 1`) to a local cache directory (`~/.octocode/repos/`). Supports sparse checkout for fetching only specific subdirectories. Cache expires after 24 hours. Returns `localPath` for use with local tools and LSP.

### Prerequisites

| Requirement | Details |
|-------------|---------|
| `ENABLE_LOCAL` | Must be `true` |
| `ENABLE_CLONE` | Must be `true` |
| `GITHUB_TOKEN` | Valid GitHub token (also via `OCTOCODE_TOKEN`, `GH_TOKEN`) |
| **git** | Must be installed and on PATH |

### Schema Parameters

| Parameter | Type | Required | Constraints | Default |
|-----------|------|----------|-------------|---------|
| `owner` | string | Yes | 1-200 chars, GitHub identifier regex | - |
| `repo` | string | Yes | 1-150 chars, GitHub identifier regex | - |
| `branch` | string | No | 1-255 chars, no `..`, no leading `-` | Default branch (API lookup) |
| `sparse_path` | string | No | 1-500 chars, relative, no `..`, no leading `-` or `/` | Full clone |
| `mainResearchGoal` | string | Yes | Research context | - |
| `researchGoal` | string | Yes | Research context | - |
| `reasoning` | string | Yes | Research context | - |

**Max queries per call: 3**

---

## Test Cases

### TC-1: Full Clone

**Goal:** Verify full repository clone without sparse path.

```json
{
  "mainResearchGoal": "Clone octocode-mcp for local analysis",
  "researchGoal": "Full shallow clone",
  "reasoning": "Full clone enables comprehensive local analysis",
  "owner": "bgauryy",
  "repo": "octocode-mcp"
}
```

**Expected:**
- [ ] Successful clone to `~/.octocode/repos/bgauryy/octocode-mcp/{branch}/`
- [ ] `localPath` returned as absolute path
- [ ] `expiresAt` shows 24-hour TTL
- [ ] `cached: false` on first call
- [ ] `.git/` directory exists in `localPath`
- [ ] Shallow clone (only 1 commit in git log)
- [ ] Helpful hints for using local tools on cloned path

---

### TC-2: Sparse Checkout

**Goal:** Verify `sparse_path` clones only a specific subdirectory.

```json
{
  "mainResearchGoal": "Clone only the tools directory",
  "researchGoal": "Sparse checkout of subdirectory",
  "reasoning": "Sparse checkout is faster for large repos",
  "owner": "bgauryy",
  "repo": "octocode-mcp",
  "sparse_path": "packages/octocode-mcp/src/tools"
}
```

**Expected:**
- [ ] Only `packages/octocode-mcp/src/tools` tree downloaded
- [ ] Cache path includes hash suffix (`__sp_{hash}`)
- [ ] `sparse_path` returned in result
- [ ] Much faster than full clone
- [ ] Files outside sparse_path are NOT present on disk

---

### TC-3: Explicit Branch

**Goal:** Verify `branch` parameter selects correct branch.

```json
{
  "mainResearchGoal": "Clone specific branch",
  "researchGoal": "Branch selection",
  "reasoning": "Branch parameter should clone specific branch",
  "owner": "bgauryy",
  "repo": "octocode-mcp",
  "branch": "main"
}
```

**Expected:**
- [ ] Cloned from `main` branch
- [ ] Cache path includes branch name (`main/`)
- [ ] `branch: "main"` in result
- [ ] Content matches `main` branch on GitHub

---

### TC-4: Default Branch Auto-Detection

**Goal:** Verify branch is auto-detected when omitted.

```json
{
  "mainResearchGoal": "Test auto branch detection",
  "researchGoal": "Clone without specifying branch",
  "reasoning": "Tool should detect default branch via GitHub API",
  "owner": "expressjs",
  "repo": "express"
}
```

**Expected:**
- [ ] Default branch auto-detected via API (e.g. `master` or `main`)
- [ ] Branch name included in response
- [ ] No error about missing branch
- [ ] `localPath` under `~/.octocode/repos/expressjs/express/{branch}/`

---

### TC-5: Sparse Clone with Branch

**Goal:** Verify sparse checkout works with explicit branch.

```json
{
  "mainResearchGoal": "Clone sparse from specific branch",
  "researchGoal": "Sparse + branch combination",
  "reasoning": "Both parameters should work together",
  "owner": "facebook",
  "repo": "react",
  "branch": "main",
  "sparse_path": "packages/react"
}
```

**Expected:**
- [ ] Only `packages/react` tree from `main` branch
- [ ] Cache path includes branch + sparse hash
- [ ] `localPath` usable by local tools
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with `localPath`, `expiresAt`, `cached`
  - [ ] Status-specific hints present
  - [ ] Hints suggest using localPath with local tools
- [ ] **Hints Validation:**
  - [ ] Hints suggest localSearchCode, localViewStructure, LSP on localPath

---

### TC-6: Different Sparse Paths Same Repo

**Goal:** Verify different sparse paths create separate cache entries.

```json
[
  {
    "mainResearchGoal": "Test separate sparse caches",
    "researchGoal": "Sparse path A",
    "reasoning": "Different sparse paths should have different caches",
    "owner": "facebook",
    "repo": "react",
    "sparse_path": "packages/react"
  },
  {
    "mainResearchGoal": "Test separate sparse caches",
    "researchGoal": "Sparse path B",
    "reasoning": "Different sparse paths should have different caches",
    "owner": "facebook",
    "repo": "react",
    "sparse_path": "packages/react-dom"
  }
]
```

**Expected:**
- [ ] Two separate cache directories (`__sp_{hash1}` vs `__sp_{hash2}`)
- [ ] Each `localPath` contains only its sparse subtree
- [ ] Both return independently valid results
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with `localPath` per query
  - [ ] Status-specific hints present
  - [ ] Hints suggest using localPath with local tools
- [ ] **Hints Validation:**
  - [ ] Hints suggest local tools on each localPath

---

### TC-7: Local Tools Integration

**Goal:** Verify cloned path works with all local tools.

```json
{
  "mainResearchGoal": "Verify local tools work on clone",
  "researchGoal": "Test local tool integration after clone",
  "reasoning": "Clone's value comes from using local + LSP tools",
  "owner": "expressjs",
  "repo": "express"
}
```

**Post-clone steps:**
1. `localViewStructure(path=localPath, depth=2)` → shows `lib/`, `test/`, `package.json`
2. `localSearchCode(pattern="middleware", path=localPath)` → finds matches
3. `localGetFileContent(path=localPath + "/package.json", fullContent=true)` → valid JSON
4. `localFindFiles(path=localPath, name="*.js", type="f")` → returns JS files

**Expected:**
- [ ] All four local tools work on the cloned path
- [ ] File content matches GitHub version
- [ ] Structure shows expected directories
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with `localPath`
  - [ ] Status-specific hints present (local tools integration)
  - [ ] Hints suggest localViewStructure, localSearchCode, localGetFileContent, localFindFiles on localPath
- [ ] **Hints Validation:**
  - [ ] Hints suggest using local tools on localPath (GOLDEN for clone workflow)

---

### TC-8: Clone → LSP Workflow

**Goal:** Verify full research funnel works on cloned repo.

```json
{
  "mainResearchGoal": "Full LSP workflow on clone",
  "researchGoal": "Clone then use LSP tools",
  "reasoning": "LSP tools should work on cloned TypeScript code",
  "owner": "bgauryy",
  "repo": "octocode-mcp"
}
```

**Post-clone steps:**
1. `localSearchCode(pattern="export function", path=localPath)` → get lineHint
2. `lspGotoDefinition(uri=file, symbolName=fn, lineHint=N)` → resolves definition
3. `lspFindReferences(uri=file, symbolName=fn, lineHint=N)` → finds usages
4. `lspCallHierarchy(uri=file, symbolName=fn, lineHint=N, direction="incoming")` → traces calls

**Expected:**
- [ ] LSP resolves definitions from cloned code
- [ ] References found across cloned files
- [ ] Call hierarchy traces function relationships
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with `localPath`
  - [ ] Status-specific hints present (LSP workflow)
  - [ ] Hints suggest localSearchCode → lspGotoDefinition → lspFindReferences → lspCallHierarchy on localPath
- [ ] **Hints Validation:**
  - [ ] Hints suggest full LSP workflow on cloned path

---

### TC-9: Security — Path Traversal

**Goal:** Verify path traversal attempts are rejected.

| Input | Expected |
|-------|----------|
| `owner="../etc"` | Schema validation error (regex rejects `..`) |
| `repo="../../passwd"` | Schema validation error |
| `sparse_path="../../etc/passwd"` | Schema validation error |
| `sparse_path="/etc/passwd"` | Schema validation error (leading `/`) |

```json
{
  "mainResearchGoal": "Security test",
  "researchGoal": "Path traversal prevention",
  "reasoning": "Ensure malicious paths are rejected",
  "owner": "../etc",
  "repo": "passwd"
}
```

**Expected:**
- [ ] All path traversal variants rejected at schema level
- [ ] Error messages do not expose internal paths
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] Status-specific hints present (validation error hints)
  - [ ] Hints suggest valid owner/repo/sparse_path format
- [ ] **Hints Validation:**
  - [ ] Hints suggest valid path format, no `..` or leading `/`

---

### TC-10: Security — Flag Injection

**Goal:** Verify git flag injection attempts are rejected.

| Input | Expected |
|-------|----------|
| `branch="--upload-pack=evil"` | Schema error (starts with `-`) |
| `sparse_path="--config=evil"` | Schema error (starts with `-`) |
| `owner="foo\\bar"` | Schema error (no backslash) |

**Expected:**
- [ ] Leading dash rejected in branch and sparse_path
- [ ] Backslash rejected in owner/repo
- [ ] Token NOT present in error messages
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] Status-specific hints present (validation error hints)
  - [ ] Hints suggest valid format (no leading `-`, no backslash)
- [ ] **Hints Validation:**
  - [ ] Hints suggest valid branch/sparse_path format

---

### TC-11: Failure — Non-Existent Targets

**Goal:** Verify graceful errors for invalid repos/branches.

| Input | Expected Error |
|-------|----------------|
| `owner="facebook", repo="nonexistent-xyz-999"` | Clone failed error |
| `branch="nonexistent-branch-xyz"` | Clone failed error |
| `sparse_path="nonexistent/deep/path"` | Empty tree or warning |

**Expected:**
- [ ] Clear error messages (not stack traces)
- [ ] Token NOT exposed in error output
- [ ] Errors isolated per query in bulk calls
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] Status-specific hints present (error hints)
  - [ ] Hints suggest repo/branch/sparse_path verification
- [ ] **Hints Validation:**
  - [ ] Hints suggest verifying repo exists, branch name, sparse_path

---

### TC-12: Failure — Missing Prerequisites

**Goal:** Verify errors when prerequisites are not met.

| Condition | Expected Error |
|-----------|----------------|
| `ENABLE_LOCAL=false` | Tool not registered / not available |
| `ENABLE_CLONE=false` | Tool not registered / not available |
| git not on PATH | "git is not installed or not on PATH" |

**Expected:**
- [ ] Clear, actionable error messages
- [ ] Tool does not appear in tool list when disabled
- [ ] **Response Validation:**
  - [ ] When tool unavailable: clear error or tool not in list
  - [ ] Hints suggest ENABLE_LOCAL, ENABLE_CLONE, git on PATH
- [ ] **Hints Validation:**
  - [ ] Hints suggest enabling prerequisites when applicable

---

### TC-13: Failure — Schema Validation

**Goal:** Verify schema rejects invalid inputs.

| Input | Expected Error |
|-------|----------------|
| Missing `owner` | Validation error: owner required |
| Missing `repo` | Validation error: repo required |
| `owner=""` | Validation error: min 1 char |
| `queries=[]` | Validation error: min 1 query |
| 4 queries | Validation error: max 3 queries |

**Expected:**
- [ ] All validation errors caught before any API/git calls
- [ ] Descriptive error messages

---

### TC-14: Bulk Queries

**Goal:** Verify multiple clones in a single call.

```json
{
  "queries": [
    {
      "mainResearchGoal": "Bulk clone test",
      "researchGoal": "Clone repo 1",
      "reasoning": "Parallel clone",
      "owner": "expressjs",
      "repo": "express"
    },
    {
      "mainResearchGoal": "Bulk clone test",
      "researchGoal": "Clone repo 2",
      "reasoning": "Parallel clone",
      "owner": "lodash",
      "repo": "lodash"
    },
    {
      "mainResearchGoal": "Bulk clone test",
      "researchGoal": "Clone repo 3",
      "reasoning": "Parallel clone",
      "owner": "bgauryy",
      "repo": "octocode-mcp"
    }
  ]
}
```

**Expected:**
- [ ] 3 result sets returned, each with `localPath`
- [ ] All three repos cloned independently
- [ ] Mixed valid/invalid queries: failures isolated per query

---

### TC-15: Edge Cases

**Goal:** Verify handling of unusual but valid inputs.

| Input | Expected |
|-------|----------|
| `owner="user.name", repo="my.repo"` | Handles dots in identifiers |
| `owner="my_org", repo="my_repo"` | Handles underscores |
| `branch="feature/new-api"` | Handles slashes in branch |
| `sparse_path="README.md"` | Sparse checkout of single file area |
| No branch (omitted) | Falls back to API default branch |

**Expected:**
- [ ] All valid edge cases accepted and clone succeeds
- [ ] Identifiers with special chars handled correctly
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with `localPath`
  - [ ] Status-specific hints present
  - [ ] Hints suggest using localPath with local tools
- [ ] **Hints Validation:**
  - [ ] Hints suggest local tools on localPath

---

### TC-16: Bulk Queries with Mixed Results (Error Isolation)

**Goal:** Verify error isolation in bulk queries — valid and invalid mixed.

```json
{
  "queries": [
    {
      "mainResearchGoal": "Bulk error test",
      "researchGoal": "Clone valid repo",
      "reasoning": "Should succeed",
      "owner": "bgauryy",
      "repo": "octocode-mcp"
    },
    {
      "mainResearchGoal": "Bulk error test",
      "researchGoal": "Clone non-existent repo",
      "reasoning": "Should fail gracefully",
      "owner": "bgauryy",
      "repo": "nonexistent-xyz-99999"
    },
    {
      "mainResearchGoal": "Bulk error test",
      "researchGoal": "Clone with bad branch",
      "reasoning": "Should fail gracefully",
      "owner": "bgauryy",
      "repo": "octocode-mcp",
      "branch": "nonexistent-branch-xyz"
    }
  ]
}
```

**Expected:**
- [ ] First query succeeds with valid `localPath`
- [ ] Second query returns clone error (repo not found)
- [ ] Third query returns clone error (branch not found)
- [ ] Token NOT exposed in any error messages
- [ ] Each result isolated — first success not affected by later failures
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with `localPath` for first query
  - [ ] Status-specific hints present (success + error hints)
  - [ ] Hints suggest local tools for success; recovery for errors
- [ ] **Hints Validation:**
  - [ ] Success hints for first; error recovery hints for second and third

---

### TC-17: Owner/Repo Max Length (Boundary)

**Goal:** Verify schema handles owner and repo at max allowed length.

| Input | Expected |
|-------|----------|
| `owner` = 200 char string | Accepted (max 200) |
| `owner` = 201 char string | Schema validation error |
| `repo` = 150 char string | Accepted (max 150) |
| `repo` = 151 char string | Schema validation error |

**Expected:**
- [ ] Values at max length accepted
- [ ] Values over max length rejected with validation error
- [ ] No crash on boundary values
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] Status-specific hints present
  - [ ] Hints suggest valid owner (max 200), repo (max 150) when validation fails
- [ ] **Hints Validation:**
  - [ ] Hints suggest length limits when validation fails

---

### TC-18: Branch With Special Characters

**Goal:** Verify branch names with slashes and dots work correctly.

```json
{
  "mainResearchGoal": "Test branch name edge cases",
  "researchGoal": "Branch with special characters",
  "reasoning": "Real-world branches often have slashes and dots",
  "owner": "bgauryy",
  "repo": "octocode-mcp",
  "branch": "improve-local-tools"
}
```

**Expected:**
- [ ] Clone succeeds with hyphenated branch name
- [ ] Cache path includes branch name
- [ ] `localPath` usable by local tools
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with `localPath`, `expiresAt`, `cached`
  - [ ] Status-specific hints present
  - [ ] Hints suggest using localPath with local tools
- [ ] **Hints Validation:**
  - [ ] Hints suggest local tools on localPath

---

### TC-19: Sparse Path to Single File Area

**Goal:** Verify `sparse_path` targeting a very specific/small directory.

```json
{
  "mainResearchGoal": "Test minimal sparse checkout",
  "researchGoal": "Sparse path to small directory",
  "reasoning": "Sparse checkout of smallest useful unit",
  "owner": "bgauryy",
  "repo": "octocode-mcp",
  "sparse_path": "docs"
}
```

**Expected:**
- [ ] Only `docs/` directory contents downloaded
- [ ] Very fast clone (small scope)
- [ ] `localPath` contains docs files
- [ ] Other directories NOT present
- [ ] **Response Validation:**
  - [ ] `instructions` field describes bulk response summary
  - [ ] `results` array contains per-query `status` (`hasResults` | `empty` | `error`)
  - [ ] `data` object present with `localPath`, `expiresAt`, `cached`
  - [ ] Status-specific hints present
  - [ ] Hints suggest using localPath with local tools
- [ ] **Hints Validation:**
  - [ ] Hints suggest local tools on localPath

---

## Scoring

| Score | Criteria |
|:-----:|:---------|
| 10 | All TC-1 through TC-19 pass. Full/sparse/cache/security/integration all work. |
| 9 | 17-18 pass; minor issue (e.g. default branch fallback) |
| 8 | 15-16 pass; sparse clone works but cache edge cases fail |
| 7 | 13-14 pass; full clone works but sparse or LSP issues |
| ≤6 | Core cloning fails, cache broken, security validation missing |

---

## Schema Edge Cases & Boundary Tests

> Schema: `queries` 1–3; `owner` 1–200 chars (GitHub identifier regex); `repo` 1–150 chars; `branch` 1–255 (no `..`, no leading `-`); `sparse_path` 1–500 chars (relative, no `..`, no leading `-` or `/`).

### TC-E1: Empty Queries Array

**Goal:** Verify empty `queries` array is rejected.

```json
{"queries": []}
```

**Expected:**
- [ ] Schema validation error: queries minItems 1

---

### TC-E2: Queries Over Max (4 queries, max is 3)

**Goal:** Verify exceeding `maxItems` on `queries` is rejected.

```json
{
  "queries": [
    {"mainResearchGoal": "t", "researchGoal": "t", "reasoning": "t", "owner": "a", "repo": "b"},
    {"mainResearchGoal": "t", "researchGoal": "t", "reasoning": "t", "owner": "a", "repo": "b"},
    {"mainResearchGoal": "t", "researchGoal": "t", "reasoning": "t", "owner": "a", "repo": "b"},
    {"mainResearchGoal": "t", "researchGoal": "t", "reasoning": "t", "owner": "a", "repo": "b"}
  ]
}
```

**Expected:**
- [ ] Schema validation error: queries maxItems is 3

---

### TC-E3: Owner Empty String

**Goal:** Verify `owner` cannot be empty (min 1 char).

```json
{
  "queries": [{
    "id": "e-owner-empty",
    "mainResearchGoal": "t",
    "researchGoal": "t",
    "reasoning": "t",
    "owner": "",
    "repo": "octocode-mcp"
  }]
}
```

**Expected:**
- [ ] Schema validation error: owner minLength / pattern

---

### TC-E4: Owner Over 200 Characters

**Goal:** Verify `owner` maxLength 200.

**Expected:**
- [ ] Schema validation error when `owner` is 201 characters (construct a valid-looking identifier repeated to exceed 200)

---

### TC-E5: Repo Empty String

**Goal:** Verify `repo` cannot be empty (min 1 char).

```json
{
  "queries": [{
    "id": "e-repo-empty",
    "mainResearchGoal": "t",
    "researchGoal": "t",
    "reasoning": "t",
    "owner": "bgauryy",
    "repo": ""
  }]
}
```

**Expected:**
- [ ] Schema validation error: repo minLength / pattern

---

### TC-E6: Repo Over 150 Characters

**Goal:** Verify `repo` maxLength 150.

**Expected:**
- [ ] Schema validation error when `repo` is 151 characters

---

### TC-E7: Branch With `..` (Traversal Attempt)

**Goal:** Verify `branch` rejects path traversal segments.

```json
{
  "queries": [{
    "id": "e-branch-dots",
    "mainResearchGoal": "t",
    "researchGoal": "t",
    "reasoning": "t",
    "owner": "bgauryy",
    "repo": "octocode-mcp",
    "branch": "main/../../etc"
  }]
}
```

**Expected:**
- [ ] Schema validation error (no `..` in branch)

---

### TC-E8: Branch With Leading `-` (Flag Injection)

**Goal:** Verify `branch` rejects leading dash.

```json
{
  "queries": [{
    "id": "e-branch-dash",
    "mainResearchGoal": "t",
    "researchGoal": "t",
    "reasoning": "t",
    "owner": "bgauryy",
    "repo": "octocode-mcp",
    "branch": "--upload-pack=evil"
  }]
}
```

**Expected:**
- [ ] Schema validation error: branch must not start with `-`

---

### TC-E9: `sparse_path` With `..` (Traversal)

**Goal:** Verify `sparse_path` rejects `..`.

```json
{
  "queries": [{
    "id": "e-sparse-dots",
    "mainResearchGoal": "t",
    "researchGoal": "t",
    "reasoning": "t",
    "owner": "bgauryy",
    "repo": "octocode-mcp",
    "sparse_path": "../../etc/passwd"
  }]
}
```

**Expected:**
- [ ] Schema validation error: sparse_path must not contain `..`

---

### TC-E10: `sparse_path` With Leading `/` (Absolute Path)

**Goal:** Verify `sparse_path` rejects absolute paths.

```json
{
  "queries": [{
    "id": "e-sparse-abs",
    "mainResearchGoal": "t",
    "researchGoal": "t",
    "reasoning": "t",
    "owner": "bgauryy",
    "repo": "octocode-mcp",
    "sparse_path": "/etc/passwd"
  }]
}
```

**Expected:**
- [ ] Schema validation error: sparse_path must be relative (no leading `/`)

---

### TC-E11: Response-Level Pagination

**Goal:** Verify root `responseCharOffset` + `responseCharLength` paginate the full MCP response for bulk clone.

```json
{
  "queries": [{
    "id": "resp",
    "mainResearchGoal": "t",
    "researchGoal": "t",
    "reasoning": "t",
    "owner": "bgauryy",
    "repo": "octocode-mcp"
  }],
  "responseCharOffset": 0,
  "responseCharLength": 3000
}
```

**Expected:**
- [ ] MCP response truncated per `responseCharLength`
- [ ] `responsePagination` metadata present (if supported for this tool)

---

### TC-E12: Duplicate Query IDs

**Goal:** Verify duplicate `id` values rejected in bulk.

```json
{
  "queries": [
    {"id": "dup", "mainResearchGoal": "t", "researchGoal": "t", "reasoning": "t", "owner": "bgauryy", "repo": "octocode-mcp"},
    {"id": "dup", "mainResearchGoal": "t", "researchGoal": "t", "reasoning": "t", "owner": "lodash", "repo": "lodash"}
  ]
}
```

**Expected:**
- [ ] Validation error: duplicate query ids

---

### TC-E13: Invalid ID Pattern

**Goal:** Verify `id` with invalid characters rejected.

```json
{
  "queries": [{
    "id": "bad id",
    "mainResearchGoal": "t",
    "researchGoal": "t",
    "reasoning": "t",
    "owner": "bgauryy",
    "repo": "octocode-mcp"
  }]
}
```

**Expected:**
- [ ] Schema validation error: id pattern `^[A-Za-z0-9._:-]+$` (or equivalent)

---

## Validation Checklist

### Core Requirements
- [ ] **All test cases use queries structure** with `mainResearchGoal`, `researchGoal`, `reasoning`
- [ ] **Response validation** — every Expected section includes explicit response checking
- [ ] **Hints validation** — every test case checks for hints (localPath usage, local tools)

### Test Cases Status

| # | Test Case | Queries | Hints | Response Validation | localPath | Status |
|---|-----------|---------|-------|---------------------|-----------|--------|
| 1 | Full clone | ✅ | ✅ | ✅ | ✅ | |
| 2 | Sparse checkout | ✅ | ✅ | ✅ | ✅ | |
| 3 | Explicit branch | ✅ | ✅ | ✅ | ✅ | |
| 4 | Default branch detection | ✅ | ✅ | ✅ | ✅ | |
| 5 | Sparse clone with branch | ✅ | ✅ | ✅ | ✅ | |
| 6 | Different sparse paths | ✅ | ✅ | ✅ | ✅ | |
| 7 | Local tools integration | ✅ | ✅ | ✅ | ✅ | |
| 8 | Clone → LSP workflow | ✅ | ✅ | ✅ | ✅ | |
| 9 | Security — path traversal | ✅ | ✅ | ✅ | - | |
| 10 | Security — flag injection | ✅ | ✅ | ✅ | - | |
| 11 | Failure — non-existent targets | ✅ | ✅ | ✅ | - | |
| 12 | Failure — missing prerequisites | ✅ | ✅ | ✅ | - | |
| 13 | Failure — schema validation | ✅ | ✅ | ✅ | - | |
| 14 | Bulk queries | ✅ | ✅ | ✅ | ✅ | |
| 15 | Edge cases | ✅ | ✅ | ✅ | ✅ | |
| 16 | Bulk queries with mixed results (error isolation) | ✅ | ✅ | ✅ | ✅ | |
| 17 | Owner/repo max length (boundary) | ✅ | ✅ | ✅ | - | |
| 18 | Branch with special characters | ✅ | ✅ | ✅ | ✅ | |
| 19 | Sparse path to single file area | ✅ | ✅ | ✅ | ✅ | |
