# Rust Offload Plan — `octocode-tools-core` → `octocode-context-utils`

> **Evidence basis:** LSP call hierarchy (5 symbols), LSP reference analysis, full source reads of all
> candidate files. Every claim is `confirmed` unless marked otherwise.
>
> **Scope:** Identify TypeScript functions in `packages/octocode-tools-core` that should be moved into
> native Rust inside `packages/octocode-context-utils`, and specify the exact API contracts.

---

## 1. What is already in Rust

The following operations are **already delegated** to native modules and are out of scope:

| Operation | Rust module | Called from |
|---|---|---|
| Content minification (all strategies) | `octocode-context-utils` | `fileContentProcess.ts`, `codeSearch.ts`, `prTransformation.ts` |
| Signature / symbol extraction | `octocode-context-utils` | `fileContentProcess.ts` |
| Semantic boundary offsets (tree-sitter) | `octocode-context-utils` | `pagination/boundary.ts` |
| YAML serialization | `octocode-context-utils` | every tool response |
| Secret detection + content sanitization | `octocode-security` | all tool output paths |
| Path + command validation | `octocode-security` | security wrapper |

---

## 2. Candidate flows — evidence summary

All candidates are in `packages/octocode-tools-core/src/`.

### 2.1 `parseRipgrepJson` — **CRITICAL**

**File:** `utils/parsers/ripgrep.ts:18–151`  
**Call chain** *(LSP confirmed, `confirmed`)*:
```
localSearchCode tool execution
  └─ executeRipgrepSearchInternal      tools/local_ripgrep/ripgrepExecutor.ts:19
       └─ parseRipgrepOutput           tools/local_ripgrep/ripgrepParser.ts:44
            └─ parseRipgrepJson        utils/parsers/ripgrep.ts:18   ← TARGET
```

**What the TypeScript does (source-verified):**

```typescript
// ripgrep.ts:24–26 — splits full NDJSON stdout into lines
const lines = jsonOutput.trim().split('\n').filter(Boolean);

// ripgrep.ts:47–103 — per-line JSON.parse + Zod safeParse
for (const line of lines) {
  const parsed = JSON.parse(line);                      // V8 JSON, per line
  const validation = RipgrepJsonMessageSchema.safeParse(parsed);  // Zod, per line
  // ... Map.set / Map.get for fileMap and contexts
}

// ripgrep.ts:114–131 — second pass: reconstruct context windows
const files = Array.from(fileMap.entries()).map(([path, entry]) => {
  const matches = entry.rawMatches.map(m => {
    // ...
    const charArray = [...value];   // UTF-16 spread — O(chars) per match
    if (charArray.length > maxLength) {
      value = charArray.slice(0, maxLength - 3).join('') + '...';
    }
  });
});
```

**Why this is the highest-priority offload:**
- ripgrep stdout is 100 KB–several MB on typical broad searches.
- `JSON.parse` on every NDJSON line is the dominant CPU cost; V8's JSON parser is
  significantly slower than `serde_json` on hot repetitive structures.
- Zod `safeParse` adds schema-validation overhead on top of every parsed object.
- The `[...value]` UTF-16 spread allocates a `char[]` for **every match snippet**
  to count Unicode code points — a linear allocation Rust handles with `chars().count()`.
- This runs on **every `localSearchCode` call** — the most-used tool.

**Rust module to create:** `packages/octocode-context-utils/src/ripgrep_parser.rs`

**Proposed napi-rs API:**
```typescript
// octocode-context-utils — new export
interface RipgrepParseOptions {
  contextLines?: number;        // default 0
  maxSnippetChars?: number;     // default 500
}

interface RipgrepMatch {
  line: number;
  column: number;
  value: string;
}

interface RipgrepFile {
  path: string;
  matchCount: number;
  matches: RipgrepMatch[];
}

interface RipgrepStats {
  matchCount?: number;
  matchedLines?: number;
  filesMatched?: number;
  filesSearched?: number;
  bytesSearched?: number;
  searchTime?: string;
}

interface RipgrepParseResult {
  files: RipgrepFile[];
  stats: RipgrepStats;
}

// Synchronous (stdout is already in memory from child_process)
export function parseRipgrepJson(
  stdout: string,
  options?: RipgrepParseOptions
): RipgrepParseResult;
```

**TypeScript side after offload:** `utils/parsers/ripgrep.ts` becomes a thin 10-line re-export
of the native call. `RipgrepJsonMessageSchema` (Zod) is deleted.

---

### 2.2 `charToByteIndex` / `byteToCharIndex` / `byteSlice` — **CRITICAL**

**File:** `utils/file/byteOffset.ts:1–45`  
**Call chain** *(LSP confirmed, `confirmed`)*:
```
applyOutputSizeLimit           utils/pagination/outputSizeLimit.ts:27    (2 call sites)
applyContentPagination         github/fileContentProcess.ts:38            (1 call site)
buildSuccessResult             tools/local_fetch_content/fetchContent.ts:521 (1 call site)
  └─ applyPagination           utils/pagination/core.ts:9
       ├─ charToByteIndex      utils/file/byteOffset.ts:19   ← TARGET (4 call sites in applyPagination)
       └─ byteToCharIndex      utils/file/byteOffset.ts:10   ← TARGET (2 call sites in applyPagination)
```

**What the TypeScript does (source-verified):**

```typescript
// byteOffset.ts:10–16 — full Buffer allocation to resolve a byte→char offset
export function byteToCharIndex(content: string, byteOffset: number): number {
  const buffer = Buffer.from(content, 'utf8');         // ← full copy of entire content
  const clampedOffset = Math.min(byteOffset, buffer.length);
  const substring = buffer.slice(0, clampedOffset).toString('utf8');
  return substring.length;
}

// byteOffset.ts:19–21 — substring allocation + byte measurement
export function charToByteIndex(content: string, charIndex: number): number {
  return Buffer.byteLength(content.substring(0, charIndex), 'utf8'); // ← substring alloc
}

// byteOffset.ts:1–7 — another full Buffer alloc for byte-range slice
export function byteSlice(content: string, byteStart: number, byteEnd: number): string {
  const buffer = Buffer.from(content, 'utf8');         // ← full copy of entire content
  return buffer.slice(byteStart, byteEnd).toString('utf8');
}
```

`applyPagination` calls `charToByteIndex` at 4 sites and `byteToCharIndex` at 2 sites
(`core.ts:55`, `56`, `68`, `69` for char; `50`, `51` for byte) — **6 allocations per page
navigation**, each proportional to full content size.

**Why this is the second-highest-priority offload:**
- Every pagination call for a large file triggers 3–6 full `Buffer.from(content, 'utf8')`
  or substring allocations, each O(content_size).
- Rust walks UTF-8 codepoint boundaries in-place: `str.char_indices().nth(n)` is O(n)
  with no allocation at all.
- `applyPagination` is called from 3 distinct hot paths covering both GitHub file content
  and local file content.

**Rust module to create:** `packages/octocode-context-utils/src/utf8_offsets.rs`

**Proposed napi-rs API:**
```typescript
// octocode-context-utils — new exports
export function charToByteOffset(content: string, charIndex: number): number;
export function byteToCharOffset(content: string, byteOffset: number): number;
export function byteSliceContent(content: string, byteStart: number, byteEnd: number): string;

// Combined: char-offset slice with optional line-boundary snapping
// Replaces both applyPagination's char/byte conversion block AND sliceByCharRespectLines
export function sliceContent(
  content: string,
  charOffset: number,
  charLength: number,
  snapToLineBoundary: boolean
): {
  text: string;
  charOffset: number;      // actual start (snapped)
  charLength: number;      // actual length
  byteOffset: number;
  byteLength: number;
  hasMore: boolean;
  nextCharOffset?: number;
};
```

**TypeScript side after offload:** `byteOffset.ts` becomes re-exports of the native calls.
`applyPagination` replaces its 6 `charToByteIndex`/`byteToCharIndex` calls with one
`sliceContent` call, simplifying the function from 80 lines to ~20.

---

### 2.3 `extractMatchingLines` — **HIGH**

**File:** `tools/local_fetch_content/contentExtractor.ts:7–121`  
**Call chain** *(LSP confirmed, `confirmed`)*:
```
processFileContentAPI     github/fileContentProcess.ts:132   (1 call site, line 221)
buildMatchExtractionState tools/local_fetch_content/fetchContent.ts:332 (1 call site, line 337)
  └─ extractMatchingLines  tools/local_fetch_content/contentExtractor.ts:7   ← TARGET
```

**What the TypeScript does (source-verified):**

```typescript
// contentExtractor.ts:36–46 — Pass 1: full scan of all lines
lines.forEach((line, index) => {
  const matches = isRegex && regex
    ? regex.test(line)                           // per-line RegExp.test
    : caseSensitive
      ? line.includes(pattern)
      : line.toLowerCase().includes(literalPattern);  // per-line toLowerCase
  if (matches) matchingLineNumbers.push(index + 1);
});

// contentExtractor.ts:49–62 — Pass 2 (fallback): if no matches, full scan again
if (!isRegex && matchingLineNumbers.length === 0) {
  lines.forEach((line, index) => {
    const haystack = stripWhitespace(caseSensitive ? line : line.toLowerCase());
    // stripWhitespace = s.replace(/\s+/g, '') — per-line regex
    if (haystack.includes(needle)) matchingLineNumbers.push(index + 1);
  });
}
```

- Input `lines` is `content.split('\n')` done at the call site
  (`fileContentProcess.ts:160`, `fetchContent.ts:336`) — that is a separate O(n) pass
  before `extractMatchingLines` even starts.
- Total: **2–3 O(n) passes** over a file that can be 100 KB–several MB.
- The `regex` crate in Rust matches in a single pass with DFA acceleration; `memchr`
  handles literal substring search with SIMD.

**Rust module to create:** `packages/octocode-context-utils/src/line_extractor.rs`

**Proposed napi-rs API:**
```typescript
// octocode-context-utils — new export
interface ExtractMatchingLinesOptions {
  isRegex?: boolean;          // default false
  caseSensitive?: boolean;    // default false
  contextLines?: number;      // default 0
  maxMatches?: number;
}

interface ExtractMatchingLinesResult {
  lines: string[];
  matchingLines: number[];
  matchCount: number;
  matchRanges: Array<{ start: number; end: number }>;
}

// Takes full file content (not pre-split) — Rust splits internally
export function extractMatchingLines(
  content: string,
  pattern: string,
  options?: ExtractMatchingLinesOptions
): ExtractMatchingLinesResult;
```

**TypeScript side after offload:** `contentExtractor.ts` becomes a thin wrapper.
The `content.split('\n')` at call sites is removed — Rust handles the split internally
in one pass with the search.

---

### 2.4 `parsePatch` / `filterPatch` / `trimDiffContext` — **MEDIUM**

**File:** `utils/parsers/diff.ts:1–162`  
**Call chain** *(LSP confirmed, `confirmed`)*:
```
applyPartialContentFilter    github/prTransformation.ts:296
  └─ filterPatch             utils/parsers/diff.ts:55
       └─ parsePatch         utils/parsers/diff.ts:8    ← TARGET
```

**What the TypeScript does (source-verified):**

```typescript
// diff.ts:8 — parsePatch: split + per-line startsWith + regex match for @@ headers
function parsePatch(patch: string): PatchLine[] {
  const lines = patch.split('\n');
  for (const line of lines) {
    if (line.startsWith('@@')) {
      const match = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
      // ...
    }
    // + / - / context classification per line
  }
}

// diff.ts:128–155 — trimDiffContext: separate patch.split('\n') (second allocation)
export function trimDiffContext(patch: string): string {
  const lines = patch.split('\n');   // ← second split, independent of parsePatch
  // builds changed Set, keep Set, final pass
}
```

**Key inefficiency:** `filterPatch` calls `parsePatch` (split #1), and `trimDiffContext`
splits again independently (split #2). A single Rust function handles both in one pass.

**When this matters:** PRs with many large files (50+ files, patches in the tens of KB range).
For small PRs the overhead is negligible.

**Rust module:** `packages/octocode-context-utils/src/diff_parser.rs`

**Proposed napi-rs API:**
```typescript
// octocode-context-utils — new export
interface FilterPatchOptions {
  additions?: number[];
  deletions?: number[];
  trimContext?: boolean;      // applies trimDiffContext logic, default false
  contextLines?: number;      // trim window, default 2
}

export function filterPatch(patch: string, options?: FilterPatchOptions): string;
```

**TypeScript side after offload:** `diff.ts` becomes a thin wrapper. `parsePatch` becomes
a private Rust function (not exported). `trimDiffContext` logic merges into `filter_patch`.

---

## 3. Dead code finding

**`sliceByCharRespectLines`** (`utils/pagination/core.ts:101–179`)

LSP reference analysis returned **1 result — the definition itself**. This function is
exported from `core.ts` but has zero callers in the entire codebase.

```typescript
// core.ts:133–137 — builds full newline index on every call
const lines: number[] = [0];
for (let i = 0; i < text.length; i++) {
  if (text[i] === '\n') lines.push(i + 1);
}
```

**Recommendation:** Delete `sliceByCharRespectLines` from TypeScript **before** implementing
the Rust `sliceContent` function (§2.2). The Rust function supersedes this logic and the
dead TypeScript code should not be ported.

---

## 4. Implementation order and placement

All Rust modules belong in `packages/octocode-context-utils/src/` following the
existing `#[napi]` pattern in `lib.rs`.

| Order | Rust module | TypeScript replaced | Priority | Hot path? |
|---|---|---|---|---|
| 1 | `ripgrep_parser.rs` + `lib.rs` exports | `utils/parsers/ripgrep.ts` (full), `RipgrepJsonMessageSchema` (Zod) | CRITICAL | Yes — every `localSearchCode` |
| 2 | `utf8_offsets.rs` + `lib.rs` exports | `utils/file/byteOffset.ts` (full), `applyPagination` char/byte block | CRITICAL | Yes — every paginated response |
| 3 | `line_extractor.rs` + `lib.rs` exports | `tools/local_fetch_content/contentExtractor.ts` (core logic) | HIGH | Yes — all file-content requests with match extraction |
| 4 | `diff_parser.rs` + `lib.rs` exports | `utils/parsers/diff.ts` (full) | MEDIUM | No — PR diff heavy paths only |
| — | Delete `sliceByCharRespectLines` | `utils/pagination/core.ts:101–179` | CLEANUP | — |

---

## 5. Validated call flows

### Flow A — localSearchCode (Rust parser replaces §2.1)

```
User: localSearchCode query
  localSearchCode tool register.ts
    executeRipgrepSearchInternal        ripgrepExecutor.ts:19
      execa ripgrep → stdout (100KB–MB)
      parseRipgrepOutput                ripgrepParser.ts:44
        parseRipgrepJson                ripgrep.ts:18       ← REPLACE with Rust
          JSON.parse × N lines
          Zod.safeParse × N lines
          [...value] spread × M matches
```

After offload:
```
        parseRipgrepJson → octocode-context-utils native (serde_json streaming)
```

### Flow B — File content pagination (Rust offsets replace §2.2)

```
applyOutputSizeLimit       outputSizeLimit.ts:27   (2 sites)
applyContentPagination     fileContentProcess.ts:38
buildSuccessResult         fetchContent.ts:521
  └─ applyPagination       core.ts:9
       ├─ charToByteIndex  byteOffset.ts:19  ← REPLACE × 4 per call
       └─ byteToCharIndex  byteOffset.ts:10  ← REPLACE × 2 per call
            Buffer.from(content, 'utf8')     ← full copy per call
```

After offload:
```
       └─ sliceContent     octocode-context-utils native (zero-copy UTF-8 walk)
```

### Flow C — File content line extraction (Rust extractor replaces §2.3)

```
processFileContentAPI      fileContentProcess.ts:132
buildMatchExtractionState  fetchContent.ts:332
  └─ extractMatchingLines  contentExtractor.ts:7   ← REPLACE
       lines.forEach(toLowerCase × lines)          Pass 1
       lines.forEach(stripWhitespace × lines)      Pass 2 (fallback)
       regex.test × lines                          RegExp per line
```

After offload:
```
  └─ extractMatchingLines  octocode-context-utils native (regex crate + memchr)
```

### Flow D — PR diff filtering (Rust parser replaces §2.4)

```
applyPartialContentFilter  prTransformation.ts:296
  └─ filterPatch            diff.ts:55
       └─ parsePatch        diff.ts:8    ← REPLACE
  └─ trimDiffContext        diff.ts:128  ← MERGE into Rust filterPatch
```

After offload:
```
  └─ filterPatch            octocode-context-utils native (single-pass diff parser)
```

---

## 6. Rust implementation notes

### Crate additions to `Cargo.toml`

```toml
# ripgrep_parser.rs
serde_json = { version = "1", features = ["preserve_order"] }  # already present via yaml

# line_extractor.rs
regex = "1"
memchr = "2"

# diff_parser.rs — no new deps (pure string processing)
```

### napi-rs patterns to follow

All new exports follow `lib.rs` conventions:
- Synchronous functions use `#[napi(js_name = "camelCase")]` on `pub fn`.
- All `String` parameters are owned at the FFI boundary (napi-rs requirement, already
  suppressed by the crate-level `#![allow(clippy::needless_pass_by_value)]`).
- Struct return types use `#[napi(object)]` (see `MinifyResult`, `YamlConversionConfig`
  as existing examples).
- Tests for FFI glue go in `lib.rs`; logic tests go in their own module file.

### TypeScript type declarations

After each Rust function is added to `lib.rs`, the napi-rs build regenerates
`index.d.ts` and `index.js` / `index.mjs`. No manual type file edits needed.

---

## 7. What NOT to offload

| File | Reason |
|---|---|
| `structureWalker.ts` | I/O-bound (`readdir`/`lstat`), not string-heavy |
| `structureParser.ts`, `structureFilters.ts`, `structureResponse.ts` | Small N, dominated by syscalls |
| `ripgrepResultBuilder.ts` | Pure orchestration on small arrays |
| `patternValidation.ts` | Runs once per query on <1 KB patterns |
| `safeRegex.ts` | Must stay aligned with JS `RegExp` semantics; patterns are tiny |
| `charSavings.ts` | Single `JSON.stringify` for metrics, not on latency path |
| `size.ts` | O(1) arithmetic |
| `pagination/boundary.ts` (JS remnant) | Heavy lifting already in Rust; remaining `snapToSemanticBoundary` O(B) is negligible |

---

*Last updated: 2026-06-15. Evidence: LSP callHierarchy + reference analysis, full source reads.*
*All `confirmed` claims have ≥2 independent sources (LSP + source read).*
