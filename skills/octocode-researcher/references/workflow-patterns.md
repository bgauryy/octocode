# Workflow Patterns

Common research flows for local and external code exploration.

---

# Part 1: Local Patterns

---

## Pattern 1: Explore-First (Unknown Codebase)

**Use when**: Entry points unclear, mixed tech, new repo.

```
localViewStructure(depth=1) → drill dirs(depth=2) → localSearchCode → localGetFileContent
```

**Steps:**
1. **Map Root**: View top-level folders (`depth: 1`)
2. **Identify Key Dirs**: Look for `src/`, `lib/`, `packages/`, `apps/`
3. **Drill Down**: Pick one relevant folder
4. **Search**: Look for architectural keywords (`App`, `Server`, `Main`, `index`)
5. **Read**: Get content of entry file

**Example Flow:**
```json
// 1. Map root
{ "path": "", "depth": 1, "summary": true }

// 2. Drill into src
{ "path": "src", "depth": 2, "details": true }

// 3. Find entry points
{ "pattern": "createServer|createApp|main", "path": "src", "filesOnly": true }

// 4. Read implementation
{ "path": "src/index.ts", "matchString": "createApp", "matchStringContextLines": 20 }
```

**Pitfall**: Diving deep without map → keep breadth-first initially.

**Success Criteria**: Understand entry point and high-level architecture.

---

## Pattern 2: Search-First (Know WHAT, not WHERE)

**Use when**: Feature name, error keyword, class/function known.

```
localSearchCode(filesOnly=true) → localGetFileContent(matchString)
```

**Steps:**
1. **Discovery**: Find files containing the term (fast)
2. **Target**: Pick the most likely file from results
3. **Read**: Read specific function with context

**Example Flow:**
```json
// 1. Discovery
{ "pattern": "AuthService", "path": "src", "type": "ts", "filesOnly": true }

// 2. Read implementation
{ "path": "src/auth/AuthService.ts", "matchString": "class AuthService", "matchStringContextLines": 30 }
```

**Pitfall**: Reading full files → prefer `matchString` + small context windows.

**Success Criteria**: Found the implementation with enough context.

---

## Pattern 3: Trace-from-Match (Follow the Trail)

**Use when**: Found definition, need impact graph (imports/usages).

```
localSearchCode(symbol) → localGetFileContent → localSearchCode(usages) → iterate
```

**Steps:**
1. **Find Definition**: Search for the symbol
2. **Read Implementation**: Get context around definition
3. **Trace Usages**: Search for imports/calls
4. **Iterate**: Follow 1-3 focused branches (cap depth to avoid explosion)

**Example Flow:**
```json
// 1. Find definition
{ "pattern": "export.*useAuth", "path": "src", "filesOnly": true }

// 2. Read implementation
{ "path": "src/hooks/useAuth.ts", "matchString": "export function useAuth", "matchStringContextLines": 25 }

// 3. Find usages
{ "pattern": "import.*useAuth|useAuth\\(", "path": "src", "filesOnly": true }

// 4. Read key usage
{ "path": "src/components/Header.tsx", "matchString": "useAuth", "matchStringContextLines": 15 }
```

**Pitfall**: Unlimited fan-out → cap depth (3 levels) and batch size (5 files).

**Success Criteria**: Understand definition + 2-3 key usages.

---

## Pattern 4: Metadata Sweep (Recent Changes)

**Use when**: Debugging recent breaks, reviewing recent areas, finding hot paths.

```
localFindFiles(modifiedWithin/size) → localSearchCode → localGetFileContent
```

**Steps:**
1. **Filter**: Find files by metadata (time, size, type)
2. **Narrow**: Search within filtered results
3. **Read**: Examine content of candidates

**Example Flow:**
```json
// 1. Find recently modified TypeScript files
{ "path": "src", "name": "*.ts", "modifiedWithin": "7d" }

// 2. Search for specific pattern in those areas
{ "pattern": "TODO|FIXME|HACK", "path": "src/auth", "contextLines": 2 }

// 3. Read suspicious file
{ "path": "src/auth/session.ts", "matchString": "HACK", "matchStringContextLines": 10 }
```

**Variations:**
```json
// Find large files that might need attention
{ "path": "", "sizeGreater": "100K", "type": "f" }

// Find config files at root
{ "path": "", "names": ["*.json", "*.yaml", "*.config.*"], "maxDepth": 1 }
```

**Pitfall**: Stopping at file names → always validate with content.

**Success Criteria**: Identified recent changes and their context.

---

## Pattern 5: Large File Inspection

**Use when**: Bundles, generated artifacts, vendor code, migrations.

```
localGetFileContent(charLength windows) → paginate with charOffset
```

**Steps:**
1. **First Window**: Read initial chunk with `charLength`
2. **Paginate**: Use `charOffset` to step through
3. **Target**: Once you find relevant section, use `matchString`

**Example Flow:**
```json
// 1. First window (bytes 0-4000)
{ "path": "dist/bundle.js", "charLength": 4000, "charOffset": 0 }

// 2. Second window (bytes 4000-8000)
{ "path": "dist/bundle.js", "charLength": 4000, "charOffset": 4000 }

// 3. Once you find area of interest, use matchString
{ "path": "dist/bundle.js", "matchString": "createRouter", "matchStringContextLines": 10 }
```

**Pitfall**: Forgetting byte-offset semantics → `charOffset`/`charLength` are BYTES not chars.

**Success Criteria**: Found and extracted relevant section from large file.

---

## Pattern 6: node_modules Inspection

**Use when**: Debugging dependency behavior, understanding library internals, finding undocumented APIs.

```
localSearchCode(noIgnore=true) → localGetFileContent → localViewStructure
```

**Steps:**
1. **Search Inside**: Use `noIgnore: true` to search node_modules
2. **Map Structure**: Understand library layout
3. **Read Source**: Get implementation details

**Example Flow:**
```json
// 1. Search inside dependency
{ "pattern": "createContext", "path": "node_modules/react", "noIgnore": true, "filesOnly": true }

// 2. View library structure
{ "path": "node_modules/react", "depth": 2, "noIgnore": true }

// 3. Read implementation
{ "path": "node_modules/react/cjs/react.development.js", "matchString": "createContext", "matchStringContextLines": 30 }
```

**Variations:**
```json
// Explore express middleware
{ "pattern": "middleware", "path": "node_modules/express/lib", "noIgnore": true }

// Find axios internals
{ "path": "node_modules/axios/lib", "depth": 2 }
```

**Pitfall**: Only reading installed version → compare with GitHub source for canonical behavior.

**Success Criteria**: Understand how dependency works internally.

---

## Pattern 7: Config Investigation

**Use when**: Understanding project configuration, build setup, environment.

```
localFindFiles(config patterns) → localGetFileContent(fullContent)
```

**Steps:**
1. **Find Configs**: Search for config file patterns
2. **Read Full**: Config files are usually small, use `fullContent`
3. **Trace References**: Find what uses these configs

**Example Flow:**
```json
// 1. Find all config files
{ "path": "", "names": ["*.config.*", "*.json", "*.yaml", ".env*"], "maxDepth": 2, "excludeDir": ["node_modules", "dist"] }

// 2. Read specific config
{ "path": "tsconfig.json", "fullContent": true }

// 3. Read build config
{ "path": "vite.config.ts", "fullContent": true }

// 4. Find what references these
{ "pattern": "tsconfig|vite.config", "path": "src", "filesOnly": true }
```

**Success Criteria**: Understand project configuration and build setup.

---

## Pattern 8: Test Coverage Investigation

**Use when**: Understanding what's tested, finding test patterns, coverage gaps.

```
localSearchCode(test patterns) → localGetFileContent → compare with source
```

**Steps:**
1. **Find Tests**: Search for test files
2. **Map Coverage**: Compare test files with source files
3. **Read Patterns**: Understand testing approach

**Example Flow:**
```json
// 1. Find test files
{ "path": "", "names": ["*.test.ts", "*.spec.ts", "*.test.tsx"], "excludeDir": ["node_modules"] }

// 2. Find source file
{ "path": "src", "name": "AuthService.ts" }

// 3. Read test for that source
{ "path": "tests/auth/AuthService.test.ts", "matchString": "describe", "matchStringContextLines": 50 }

// 4. Compare: does test cover main functions?
{ "pattern": "it\\(|test\\(", "path": "tests/auth/AuthService.test.ts" }
```

**Success Criteria**: Understand test coverage and patterns.

---

## Pattern 9: Monorepo Navigation

**Use when**: Working in monorepo with multiple packages.

```
localViewStructure(packages) → localSearchCode(cross-package) → trace dependencies
```

**Steps:**
1. **Map Packages**: View packages/apps directory
2. **Understand Dependencies**: Check package.json files
3. **Cross-Package Search**: Find shared code usage

**Example Flow:**
```json
// 1. Map monorepo structure
{ "path": "packages", "depth": 1 }

// 2. View specific package
{ "path": "packages/core", "depth": 2 }

// 3. Read package deps
{ "path": "packages/app/package.json", "fullContent": true }

// 4. Find cross-package imports
{ "pattern": "from '@myorg/core'", "path": "packages/app/src", "filesOnly": true }
```

**Success Criteria**: Understand package relationships and shared code.

---

## Pattern 10: Error Investigation

**Use when**: Debugging errors, tracing error sources.

```
localSearchCode(error message) → trace throw/catch → find root cause
```

**Steps:**
1. **Find Error Source**: Search for error message
2. **Trace Throws**: Find where error is thrown
3. **Trace Catches**: Find error handlers
4. **Root Cause**: Understand triggering conditions

**Example Flow:**
```json
// 1. Find error message
{ "pattern": "Invalid credentials", "path": "src", "contextLines": 5 }

// 2. Find throw statements
{ "pattern": "throw.*Error.*Invalid", "path": "src", "filesOnly": true }

// 3. Read throw context
{ "path": "src/auth/validate.ts", "matchString": "throw new Error", "matchStringContextLines": 15 }

// 4. Find callers
{ "pattern": "validateCredentials|validate\\(", "path": "src", "filesOnly": true }
```

**Success Criteria**: Found error source and triggering conditions.

---

## The Verification Gate

**BEFORE claiming a finding, pass this gate:**

1. **IDENTIFY**: What exact lines of code prove this?
2. **FETCH**: Did you read the *actual file content*? (Search snippets don't count)
3. **VERIFY**: Does the logic actually do what you think?
4. **CONTEXT**: Is this the currently active code (not deprecated/test)?
5. **ONLY THEN**: Output the finding.

---

## Anti-Patterns (Local)

| Bad | Good |
|-----|------|
| **Citing Search Results** | **Citing File Content** (Read the file!) |
| **"I assume..."** | **"The code shows..."** |
| **"Should work like..."** | **"Logic implements..."** |
| **Broad Search (`auth`)** | **Targeted Search (`class AuthService`)** |
| **Shell commands** | **Local tools with pagination** |
| **Full file dumps** | **matchString + context** |
| **Guessing paths** | **Verify with structure first** |
| **Ignoring hints** | **Follow tool hints** |

---

# Part 2: External Patterns

## Pattern 11: Package Discovery

**Use when**: Finding the right library, comparing packages, evaluating options.

```
npmSearch(keyword) → ghViewRepoStructure → ghGetFileContent(README)
```

**Steps:**
1. **Search**: Find packages matching use case
2. **Evaluate**: Check stars, downloads, last update
3. **Explore**: View repo structure of top candidates
4. **Read**: Check README, examples, source quality

**Example Flow:**
```json
// 1. Search packages
{ "query": "rate-limiter express", "registry": "npm" }

// 2. View top result repo
{ "owner": "express-rate-limit", "repo": "express-rate-limit", "depth": 2 }

// 3. Read README
{ "owner": "express-rate-limit", "repo": "express-rate-limit", "path": "README.md" }

// 4. Read source
{ "owner": "express-rate-limit", "repo": "express-rate-limit", "path": "src/lib.ts", "matchString": "export" }
```

**Pitfall**: Stopping at npm metadata → always read the actual source.

**Success Criteria**: Found best package with evidence (stars, maintenance, API quality).

---

## Pattern 12: External Repo Exploration

**Use when**: Understanding how another project implements something.

```
ghSearchRepos → ghViewRepoStructure → ghSearchCode → ghGetFileContent
```

**Steps:**
1. **Find Repo**: Search by topic/keyword or use known repo
2. **Map Structure**: View repo layout
3. **Search Pattern**: Find the specific implementation
4. **Read Source**: Get implementation details

**Example Flow:**
```json
// 1. Find repo (skip if known)
{ "query": "nextjs authentication", "sort": "stars" }

// 2. Map structure
{ "owner": "vercel", "repo": "next.js", "depth": 1 }

// 3. Search for auth patterns
{ "query": "getServerSession", "owner": "vercel", "repo": "next.js", "path": "packages" }

// 4. Read implementation
{ "owner": "vercel", "repo": "next.js", "path": "packages/next/src/server/auth.ts", "matchString": "getServerSession" }
```

**Pitfall**: Searching too broadly → narrow to specific owner/repo ASAP.

**Success Criteria**: Found specific implementation with line references.

---

## Pattern 13: Dependency Source Investigation

**Use when**: Need to understand how an imported library works internally.

```
npmSearch → get repo URL → ghViewRepoStructure → ghSearchCode → ghGetFileContent
```

**Steps:**
1. **Find Package**: Get repo URL from package metadata
2. **Map Layout**: Understand library structure
3. **Search**: Find the exported function/class
4. **Read**: Get the implementation

**Example Flow:**
```json
// 1. Find package repo URL
{ "query": "zod", "registry": "npm" }

// 2. View structure
{ "owner": "colinhacks", "repo": "zod", "depth": 2 }

// 3. Search for specific feature
{ "query": "z.object", "owner": "colinhacks", "repo": "zod", "path": "src" }

// 4. Read implementation
{ "owner": "colinhacks", "repo": "zod", "path": "src/types.ts", "matchString": "object(" }
```

**Pitfall**: Only reading installed node_modules version → canonical GitHub source shows latest intent.

**Success Criteria**: Understand how the dependency works at the source level.

---

## Pattern 14: PR Archaeology

**Use when**: Understanding why code changed, tracing regression origins, reviewing decisions.

```
ghSearchPRs → read PR files → trace changes
```

**Steps:**
1. **Search PRs**: Find merged PRs matching the topic
2. **Read Context**: Get PR description and changed files
3. **Trace Changes**: Read specific files at that point in time

**Example Flow:**
```json
// 1. Find relevant PRs
{ "query": "fix auth token refresh", "owner": "myorg", "repo": "backend", "state": "merged" }

// 2. Read changed files from PR
{ "owner": "myorg", "repo": "backend", "path": "src/auth/tokenRefresh.ts", "matchString": "refreshToken" }
```

**Pitfall**: Reading only PR titles → read the actual changed files.

**Success Criteria**: Understood the change motivation and implementation.

---

## Pattern 15: Cross-Boundary Research (Local + External)

**Use when**: Local code uses external library, need to understand both sides.

```
LOCAL: localSearchCode(import) → lspGotoDefinition
EXTERNAL: npmSearch → ghSearchCode → ghGetFileContent
MERGE: Compare and document
```

**Steps:**
1. **Local**: Find how the dependency is used locally
2. **External**: Find how the dependency works at source
3. **Merge**: Compare usage with intended API

**Example Flow:**
```json
// LOCAL — find usage
{ "pattern": "import.*from 'next-auth'", "path": "src", "filesOnly": true }
// → then lspGotoDefinition on the import, lspFindReferences for all usages

// EXTERNAL — find source
{ "query": "next-auth", "registry": "npm" }
// → then ghViewRepoStructure + ghSearchCode + ghGetFileContent

// MERGE: Is local usage correct? Are there missed features? Breaking changes?
```

**Pitfall**: Researching one side only → always trace both local usage AND external implementation.

**Success Criteria**: Understand both sides and their interaction.

---

## Anti-Patterns (External)

| Bad | Good |
|-----|------|
| **`gh api` for GitHub** | **`ghSearchCode` / `ghGetFileContent`** |
| **`WebFetch` for GitHub** | **Octocode GitHub tools** |
| **`npm search` in shell** | **`npmSearch`** |
| **Guessing owner/repo** | **`npmSearch` or `ghSearchRepos` first** |
| **Reading entire large files** | **`matchString` targeting** |
| **Broad GitHub search** | **Narrow to owner/repo ASAP** |
| **Skipping structure** | **`ghViewRepoStructure` before reading** |

---

# Checklists

## Local Research Checklist

Before completing local research:

- [ ] **Goal defined?** (Atomic question)
- [ ] **Code evidence found?** (Line numbers/paths)
- [ ] **The Gate passed?** (Read full content)
- [ ] **Cross-referenced?** (Imports/Usage)
- [ ] **Gaps documented?**
- [ ] **User checkpoint offered?** (Continue/Save)

## External Research Checklist

Before completing external research:

- [ ] **Repo/package found via search?** (Not guessed)
- [ ] **Structure explored first?** (`ghViewRepoStructure`)
- [ ] **References include full GitHub URLs?** (With line numbers)
- [ ] **Source verified?** (Read actual code, not just metadata)
- [ ] **Gaps documented?**
- [ ] **User checkpoint offered?** (Continue/Save)

