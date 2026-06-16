# Tools Reference

Complete parameter reference for Octocode tools (Local + External).

**Required Fields (ALL queries)**: `mainResearchGoal`, `researchGoal`, `reasoning`

---

# Part 1: Local Tools

---

## localViewStructure

Explore local directory structure with filtering and sorting.

```typescript
{
  path: string;                    // Directory path (required)
  depth?: number;                  // 1-5 (default 1)
  entriesPerPage?: number;         // ≤20
  entryPageNumber?: number;        // Pagination
  details?: boolean;               // Show file sizes
  hidden?: boolean;                // Include dotfiles
  extensions?: string[];           // Filter by extension
  pattern?: string;                // Filter by name pattern
  filesOnly?: boolean;
  directoriesOnly?: boolean;
  sortBy?: "name" | "size" | "time" | "extension";
  summary?: boolean;               // Include summary stats
  showFileLastModified?: boolean;  // Show modification times
}
```

**Tips:**
- Start `depth: 1` at root, drill with `depth: 2` on specific dirs
- Use `details: true` to see file sizes
- Check `packages/` or `apps/` for monorepos
- Use `sortBy: "time"` to find recently modified

**Example Queries:**
```json
// Map root structure
{ "path": "", "depth": 1, "summary": true }

// Drill into src with file details
{ "path": "src", "depth": 2, "details": true }

// Find TypeScript files only
{ "path": "src", "extensions": ["ts", "tsx"], "filesOnly": true }

// Show recently modified
{ "path": "", "sortBy": "time", "showFileLastModified": true }
```

---

## localSearchCode

Fast pattern search with discovery mode and pagination.

```typescript
{
  pattern: string;                 // Search pattern (required)
  path: string;                    // Search root (required)
  filesOnly?: boolean;             // Discovery mode - just list files
  type?: string;                   // "ts", "py", "js" etc
  include?: string[];              // Glob patterns to include
  exclude?: string[];              // Glob patterns to exclude
  excludeDir?: string[];           // Directories to skip
  noIgnore?: boolean;              // Search inside node_modules
  matchesPerPage?: number;         // 1-100
  filesPerPage?: number;           // ≤20
  filePageNumber?: number;         // Pagination
  contextLines?: number;           // Lines around match
  caseSensitive?: boolean;
  caseInsensitive?: boolean;
  smartCase?: boolean;             // Default: true (case-sensitive if pattern has uppercase)
  wholeWord?: boolean;
  multiline?: boolean;
  fixedString?: boolean;           // Treat pattern as literal string
  mode?: "discovery" | "paginated" | "detailed";
  matchContentLength?: number;     // Max chars per match (default 200)
}
```

**Tips:**
- **Start with `filesOnly: true`** for discovery (fast, token-efficient)
- Use `noIgnore: true` to search inside `node_modules`
- Use `type` for common file extensions
- Returns `location.charOffset/charLength` as BYTE offsets (ripgrep)
- Use `fixedString: true` for special characters in pattern

**Discovery vs Content Search:**
```json
// Discovery mode - find files containing pattern (fast)
{ "pattern": "AuthService", "path": "src", "filesOnly": true }

// Content search - get match context
{ "pattern": "AuthService", "path": "src", "contextLines": 3 }
```

**node_modules Inspection:**
```json
// Search inside React
{ "pattern": "createContext", "path": "node_modules/react", "noIgnore": true }

// Search specific dependency
{ "pattern": "Router", "path": "node_modules/react-router", "noIgnore": true, "filesOnly": true }
```

**Filtering Examples:**
```json
// TypeScript files only
{ "pattern": "export", "path": "src", "type": "ts" }

// Exclude test files
{ "pattern": "fetchData", "path": "src", "exclude": ["*.test.ts", "*.spec.ts"] }

// Exclude directories
{ "pattern": "config", "path": "", "excludeDir": ["node_modules", "dist", "coverage"] }
```

---

## localGetFileContent

Read local file content with targeted extraction.

```typescript
{
  path: string;                    // File path (required)
  
  // Choose ONE strategy:
  matchString?: string;            // Pattern to find
  matchStringContextLines?: number; // 1-50 lines of context
  matchStringIsRegex?: boolean;
  matchStringCaseSensitive?: boolean;
  
  charOffset?: number;             // BYTE offset for pagination
  charLength?: number;             // BYTES to read (1000-4000 recommended)
  
  startLine?: number;              // Line range
  endLine?: number;
  
  fullContent?: boolean;           // Small files only (<10KB)
}
```

**Tips:**
- **Prefer `matchString` over `fullContent`** for token efficiency
- `charOffset`/`charLength` are BYTES, not characters
- Use `charLength: 2000-4000` for pagination windows
- Large files require `charLength` or `matchString`

**Strategy Selection:**
| Scenario | Strategy | Example |
|----------|----------|---------|
| Know function name | `matchString` | `{ matchString: "export function useAuth", matchStringContextLines: 20 }` |
| Know exact lines | `startLine/endLine` | `{ startLine: 1, endLine: 50 }` |
| Small config files | `fullContent` | `{ fullContent: true }` |
| Large file pagination | `charLength/charOffset` | `{ charLength: 3000, charOffset: 0 }` |

**Example Queries:**
```json
// Find function with context
{ "path": "src/auth/useAuth.ts", "matchString": "export function useAuth", "matchStringContextLines": 25 }

// Read first 50 lines
{ "path": "src/index.ts", "startLine": 1, "endLine": 50 }

// Read small config
{ "path": "package.json", "fullContent": true }

// Paginate large file (first window)
{ "path": "dist/bundle.js", "charLength": 4000, "charOffset": 0 }

// Paginate large file (next window)
{ "path": "dist/bundle.js", "charLength": 4000, "charOffset": 4000 }
```

---

## localFindFiles

Find files by metadata (name, time, size, permissions).

```typescript
{
  path: string;                    // Search root (required)
  name?: string;                   // "*.ts" pattern
  iname?: string;                  // Case-insensitive name
  names?: string[];                // Multiple patterns
  type?: "f" | "d" | "l";          // file/directory/link
  modifiedWithin?: string;         // "7d", "2h", "30m"
  modifiedBefore?: string;
  accessedWithin?: string;
  sizeGreater?: string;            // "10M", "1K"
  sizeLess?: string;
  excludeDir?: string[];
  maxDepth?: number;               // 1-10
  minDepth?: number;               // 0-10
  filesPerPage?: number;           // ≤20
  filePageNumber?: number;
  details?: boolean;               // Show file metadata
  showFileLastModified?: boolean;  // Default: true
  pathPattern?: string;            // Path pattern match
  regex?: string;                  // Regex file name match
  empty?: boolean;                 // Find empty files
  executable?: boolean;            // Find executable files
}
```

**Tips:**
- Results sorted by modified time (most recent first)
- Use `modifiedWithin: "7d"` to find recent changes
- Combine with `localGetFileContent` or `localSearchCode` for content
- Time suffixes: `d` (days), `h` (hours), `m` (minutes)
- Size suffixes: `K` (kilobytes), `M` (megabytes), `G` (gigabytes)

**Example Queries:**
```json
// Recent TypeScript changes
{ "path": "src", "name": "*.ts", "modifiedWithin": "7d" }

// Large files needing attention
{ "path": "", "sizeGreater": "100K", "type": "f" }

// Config files
{ "path": "", "names": ["*.json", "*.yaml", "*.yml"], "maxDepth": 2 }

// Empty files (potential issues)
{ "path": "src", "empty": true, "type": "f" }

// Recently accessed
{ "path": "src", "accessedWithin": "1d" }
```

---

## Query Batching

Parallelize independent searches (max 5 queries per call):

```json
{
  "queries": [
    { "pattern": "AuthService", "path": "src", "filesOnly": true },
    { "pattern": "authMiddleware", "path": "src", "filesOnly": true },
    { "pattern": "AuthUser", "path": "src", "filesOnly": true },
    { "pattern": "useAuth", "path": "src", "filesOnly": true },
    { "pattern": "authContext", "path": "src", "filesOnly": true }
  ]
}
```

**Benefits:**
- Single network round-trip
- Parallel execution
- Combined results with hints

---

# Part 2: External Tools (GitHub & Packages)

## ghSearchCode

Search code across GitHub repositories.

```typescript
{
  query: string;          // Search query (required) — GitHub code search syntax
  owner?: string;         // Filter by repo owner
  repo?: string;          // Filter by repo name (requires owner)
  language?: string;      // Filter by language
  path?: string;          // Filter by file path
  extension?: string;     // Filter by file extension
  filename?: string;      // Filter by filename
  resultsPerPage?: number; // Results per page
  pageNumber?: number;    // Page number
}
```

**Tips:**
- Narrow to `owner`/`repo` ASAP for faster, more relevant results
- Use `path` to scope searches (e.g., `path:src/auth`)
- Use `extension` to filter by file type
- Combine with `ghGetFileContent` to read matched files

---

## ghSearchRepos

Find repositories by topic, language, stars.

```typescript
{
  query: string;          // Search query (required)
  language?: string;      // Filter by language
  sort?: "stars" | "forks" | "updated"; // Sort order
  order?: "asc" | "desc";
  resultsPerPage?: number;
  pageNumber?: number;
}
```

**Tips:**
- Use for discovering repos when you don't know the exact name
- Sort by `stars` for popular/battle-tested repos
- Sort by `updated` for actively maintained repos

---

## ghViewRepoStructure

Explore external repo directory layout.

```typescript
{
  owner: string;          // Repo owner (required)
  repo: string;           // Repo name (required)
  branch?: string;        // Branch name (default: main/master)
  path?: string;          // Sub-path within repo
  depth?: number;         // Directory depth (1-5)
}
```

**Tips:**
- Use BEFORE `ghGetFileContent` to understand layout
- Start `depth: 1` at root, drill with `depth: 2` on specific dirs
- Identify `src/`, `lib/`, `packages/` for entry points

---

## ghGetFileContent

Read files from external GitHub repos.

```typescript
{
  owner: string;          // Repo owner (required)
  repo: string;           // Repo name (required)
  path: string;           // File path (required)
  branch?: string;        // Branch name
  matchString?: string;   // Pattern to find within file
  matchStringContextLines?: number; // Lines around match
}
```

**Tips:**
- Use `matchString` for large files (avoid reading entire file)
- Use LAST in the external flow — after search/structure exploration
- Generate full GitHub URLs for references: `https://github.com/{owner}/{repo}/blob/{branch}/{path}`

---

## ghSearchPRs

Search PRs by query, state, labels.

```typescript
{
  query: string;          // Search query (required)
  owner?: string;         // Repo owner
  repo?: string;          // Repo name
  state?: "open" | "closed" | "merged";
  sort?: "created" | "updated" | "comments";
  order?: "asc" | "desc";
  resultsPerPage?: number;
  pageNumber?: number;
}
```

**Tips:**
- Use `state: "merged"` for understanding change history
- Combine with `ghGetFileContent` to read files from specific commits
- Narrow to `owner`/`repo` for relevant results

---

## npmSearch

Search npm packages by name or keyword.

```typescript
{
  query: string;          // Package name or keyword (required)
  resultsPerPage?: number;
}
```

**Tips:**
- Use to find repo URL for a package, then follow with `ghViewRepoStructure`
- Returns package metadata: name, description, version, repo URL, downloads
- Use FIRST when investigating an external dependency

---

## Query Batching (External)

GitHub tools support batching (max 3 queries per call):

```json
{
  "queries": [
    { "query": "useState", "owner": "facebook", "repo": "react" },
    { "query": "useEffect", "owner": "facebook", "repo": "react" }
  ]
}
```
