# Octocode MCP tool improvements — dynamic agentic evidence plan

Benchmark session: `20260525-173417`.

Octocode was **very token-efficient** but often **too shallow**. The failure pattern was not “tools need to dump more code.” It was: tools returned evidence, but the agent did not always know whether the evidence was complete, answer-ready, current, implementation-level, or only a hint.

This plan avoids rigid, breakable suggestions. It focuses on:

1. **Returned schemas** — add flexible metadata that tells the agent what the result means.
2. **Tool instructions/descriptors** — guide agents toward better follow-up decisions.
3. **Missing response data** — add small high-value fields when current responses omit necessary facts.
4. **Dynamic evidence contracts** — support code, docs, configs, manifests, PRs, issues, package metadata, and repo metadata without hard-coding one language or framework.

No recommendation here should require benchmark-specific behavior or one-language-only parsing. Any extraction should be best-effort, opt-in, and degrade gracefully to current behavior.

---

## Core direction

### What to avoid

- Rigid closed enums that only fit this benchmark.
- TypeScript/JavaScript-only assumptions for tools that should work across languages.
- Heavy AST-specific implementation as the primary fix.
- Tool behavior that silently hides raw evidence.
- Large default outputs.
- Overfitted “last merged PR” or “package list” logic that cannot generalize.

### What to prefer

- Flexible request objects such as:

```ts
intent?: string;
extract?: {
  profile?: string;
  target?: string;
  fields?: string[];
  maxItems?: number;
};
```

- Response metadata such as:

```ts
evidenceKind?: string;
answerReady?: boolean;
missingFields?: string[];
confidence?: 'high' | 'medium' | 'low';
nextQueries?: Array<{ tool: string; reason: string; suggestedInput: unknown }>;
```

- Small universal fields that unlock correctness, e.g. `mergedAt`, exact repo metrics, file path, line, title, URL, changed files, truncation/completeness.

---

## Priority order

| Priority | Work | Reason |
|---|---|---|
| P0 | Add answer-readiness + missing-fields metadata across tools | Prevents placeholder answers and tells the agent when to continue. |
| P0 | Add missing PR/repo fields to responses, especially `mergedAt` | Fixes current/drift questions with tiny token cost. |
| P0 | Add flexible extraction request/response shape to content tools | Supports code, docs, configs, manifests, and plain text. |
| P0 | Add factual grouping/completeness to structure tools | Prevents architecture summaries without exact names. |
| P1 | Add match/reference classification and next-query hints to search/LSP tools | Makes agentic follow-up safer without hardcoded flows. |

---

## Cross-tool response contract

All tools should be able to return this kind of lightweight metadata when useful:

```yaml
evidence:
  kind: metadata | content | structure | code | docs | config | pr | repo | package | references | calls
  answerReady: true | false
  confidence: high | medium | low
  missingFields: []
  complete: true | false
  reason: short explanation
nextQueries:
  - tool: githubGetFileContent
    reason: Need exact file content for matched config key
    suggestedInput: {...}
```

**Reason:** Agentic systems improve when tools say what evidence means, not only what text matched.

**Pros:** General across tools and content types; small; reduces hallucination.  
**Cons:** Requires consistent definitions and tests so metadata does not become noisy.

---

## 1. `githubSearchCode`

**Current issue:** Search returns compact matches, but the agent may not know if a match is runtime code, docs, config, tests, comments, examples, generated files, or a wrapper/declaration.

### Improvements

| Area | Recommendation |
|---|---|
| Returned schema | Add optional match metadata: `evidenceKind`, `matchRole`, `languageOrFormat`, `isLikelyGenerated`, `isTest`, `isExample`, `isDocs`, `answerReady`, `nextQueries`. |
| Instructions/descriptors | Tell agents that search results are candidates, not final proof. They should continue when a match is only docs/config/example/wrapper or when `answerReady=false`. |
| Missing data | Include enough local context to identify the matched entity: path, line, matched text, nearby names/keys/headings, and suggested next search terms when cheap. |

**Reason:** The tool should help the agent decide whether a match is authoritative enough for the question.

**Pros:** Works for all languages and text formats; avoids hardcoded implementation tracing; improves follow-up.  
**Cons:** Classification is approximate; descriptor must warn that metadata is a hint, not proof.

---

## 2. `githubGetFileContent`

**Current issue:** Raw slices are useful, but exact answers often need structured facts from many formats: code declarations, Markdown headings, config keys, manifest fields, tables, links, or changelog entries.

### Improvements

| Area | Recommendation |
|---|---|
| Returned/request schema | Add flexible `extract?: { profile?: string; target?: string; fields?: string[]; maxItems?: number }`. Profiles are open-ended hints, not rigid modes. |
| Instructions/descriptors | Say extraction is best-effort and format-aware. Agents should request extraction for exact facts and raw slices for quotation/context. |
| Missing data | Return `contentType`, `detectedFormat`, `matchedSections`, `missingFields`, `complete`, and `answerReady`. |

**Reason:** The same tool should help with source files, README/API docs, JSON/YAML/TOML config, package manifests, lockfiles, and text files.

**Pros:** General and future-proof; keeps default output compact; avoids language-specific brittle behavior.  
**Cons:** Extraction quality varies by format; open profiles need clear documentation.

---

## 3. `githubViewRepoStructure`

**Current issue:** Tree output can lead to vague architecture summaries. Agents need exact factual groups and completeness state.

### Improvements

| Area | Recommendation |
|---|---|
| Returned/request schema | Add flexible `viewIntent?: string` and response groups like `facts.directories`, `facts.files`, `facts.manifests`, `facts.docs`, `facts.configs`, `facts.tests`, `facts.examples`, `facts.possiblePackageRoots`. |
| Instructions/descriptors | Require exact names before interpretation. Inference should be marked separately from facts. |
| Missing data | Return `complete`, `truncatedReason`, `nextPageQuery`, and `inferenceConfidence`. |

**Reason:** Repo structure questions vary: packages, docs, configs, examples, tests, CI, generated files, deployment files. A flexible grouping response is safer than rigid package-only modes.

**Pros:** More factual answers across repo types; avoids overfitted package assumptions.  
**Cons:** Grouping is heuristic and must not hide raw tree entries.

---

## 4. `githubSearchRepositories`

**Current issue:** The tool can return metrics, but known-repo comparison and current-state answers need exact, preserved fields.

### Improvements

| Area | Recommendation |
|---|---|
| Returned/request schema | Add exact lookup/comparison support for known repos using flexible `fields?: string[]`. |
| Instructions/descriptors | Distinguish discovery search from exact metadata lookup. Tell agents to report values, not meta-instructions. |
| Missing data | Preserve requested fields in response/TSV: stars, forks, pushedAt, updatedAt, openIssuesCount, defaultBranch, language, license, archived, URL where available. |

**Reason:** Current-state questions fail when the agent receives or needs metrics but does not report them.

**Pros:** Broadly useful for any repo comparison; tiny token footprint.  
**Cons:** Provider field support differs; response should mark unavailable fields instead of guessing.

---

## 5. `githubSearchPullRequests`

**Current issue:** Major benchmark failure area. PR questions need exact identity and timeline fields. Missing or hidden `mergedAt` caused bad “last merged” answers.

### Improvements

| Area | Recommendation |
|---|---|
| Returned/request schema | Add flexible `intent?: string` and `fields?: string[]`, e.g. identity, timeline, changedFiles, reviewSignals, commentsSummary, diffSummary. Avoid benchmark-only presets as the only interface. |
| Instructions/descriptors | For merged/current PR questions, require number/title/state/mergedAt/URL. For review questions, anchor on title and changed files before summarizing. |
| Missing data | Add `mergedAt` to default compact output. Always preserve PR identity: number, title, URL, state, createdAt, updatedAt, mergedAt when available. |

**Reason:** PR synthesis varies, but PR identity and timeline fields are universally useful.

**Pros:** High correctness gain with small data; flexible enough for PR lists, reviews, archaeology, and status checks.  
**Cons:** Review/comment summaries can be expensive; use bounded fields and mark missing/unfetched data.

---

## 6. `packageSearch`

**Current issue:** Package results should guide agentic follow-up without assuming one ecosystem layout.

### Improvements

| Area | Recommendation |
|---|---|
| Returned/request schema | Add optional `fields?: string[]` and `includeHints?: boolean` for repo, docs, homepage, source entries, manifest fields, deprecation, latest version. |
| Instructions/descriptors | Position as package metadata + source discovery bridge. Use before broad repo search for named packages. |
| Missing data | Return confidence for repo URL and source/docs hints; mark missing metadata explicitly. |

**Reason:** Packages can point to repos, docs, exports, CLI entries, types, and ecosystem metadata, but those hints vary by package manager.

**Pros:** Reduces search loops; supports npm/PyPI now and future ecosystems.  
**Cons:** Metadata can be stale; hints should not be treated as proof.

---

## 7. `localSearchCode`

**Current issue:** Local search gives matches and line hints, but the agent still must infer whether to use LSP, content extraction, or another search.

### Improvements

| Area | Recommendation |
|---|---|
| Returned schema | Add optional `matchRole`, `evidenceKind`, `detectedFormat`, `nextQueries`, and LSP-ready hints only when appropriate. |
| Instructions/descriptors | Use search as evidence discovery. Use LSP only for supported source files and semantic questions; use content extraction for docs/config/text. |
| Missing data | Include line, path, matched text, nearby names/keys/headings, and whether the match appears in docs/tests/examples/generated files. |

**Reason:** Local search applies to every text file type, not just code. The tool should route the agent dynamically.

**Pros:** Better next-tool selection; fewer bad LSP calls; still compact.  
**Cons:** Classification is heuristic and should be low-confidence when uncertain.

---

## 8. `localGetFileContent`

**Current issue:** Same as remote content: raw slices are not always best for exact facts across code/docs/configs.

### Improvements

| Area | Recommendation |
|---|---|
| Returned/request schema | Same flexible `extract` object as `githubGetFileContent`. |
| Instructions/descriptors | Extraction is format-aware and best-effort; raw read is for exact quote/context. |
| Missing data | Return detected format, matched sections, answer-readiness, completeness, and missing fields. |

**Reason:** Local and remote content tools should have the same mental model.

**Pros:** Consistent API; supports all file types; avoids unnecessary full-file reads.  
**Cons:** Extractor behavior must be transparent about confidence.

---

## 9. `localViewStructure`

**Current issue:** It lists entries but does not expose enough project-shape metadata for agent decisions.

### Improvements

| Area | Recommendation |
|---|---|
| Returned/request schema | Add flexible `viewIntent` and grouped facts similar to remote structure. |
| Instructions/descriptors | Exact names first, inferred roles second. Do not assume JS workspaces or one repo style. |
| Missing data | Return completeness/truncation, grouped roles, and next drill-down queries. |

**Reason:** Local repo structure can include any language, docs, configs, datasets, generated artifacts, or deployment files.

**Pros:** Better onboarding and exploration answers.  
**Cons:** Role grouping is approximate.

---

## Recommended implementation style

### 1. Start with response-only changes where possible

Safest first changes:

- Add `mergedAt` to PR compact output.
- Preserve exact repo metadata fields in repo comparison/discovery output.
- Add `answerReady`, `missingFields`, `complete`, `confidence` to finalizers where the tool already knows this.
- Add `nextQueries` hints when the next step is obvious and cheap.

### 2. Add flexible request fields second

Add generic fields that do not lock the design:

```ts
intent?: string;
fields?: string[];
extract?: {
  profile?: string;
  target?: string;
  maxItems?: number;
};
includeHints?: boolean;
```

### 3. Keep extraction best-effort

Extraction should be layered:

1. Use known structured data already returned by providers.
2. Use format-aware lightweight scanners for common formats.
3. Fall back to bounded raw content and mark confidence/missing fields.

### 4. Do not hide raw evidence

Structured facts should supplement raw paths/snippets/URLs, not replace them. Agents need citations and escape hatches.

## Suggestion ratings

Scale:

- **Impact**: 1 low → 5 high benchmark/user-value gain.
- **Effort**: 1 small → 5 large implementation/test cost.
- **Risk**: 1 safe → 5 likely to add noise, brittleness, or provider inconsistency.
- **Priority score**: practical build priority, not a math average.

| Suggestion | Impact | Effort | Risk | Priority | Rating | Reason |
|---|---:|---:|---:|---|---|---|
| Add missing PR/repo fields, especially `mergedAt` | 5 | 1 | 1 | P0 | **A+** | Tiny schema/output addition with direct benchmark correctness gain. |
| Add cross-tool `answerReady`, `missingFields`, `complete`, `confidence` metadata | 5 | 2 | 2 | P0 | **A** | Solves placeholder answers generally; needs consistent semantics to avoid noisy hints. |
| Preserve requested exact repo metadata fields and support exact repo lookup/comparison | 5 | 2 | 2 | P0 | **A** | Directly fixes current-state comparisons; provider field differences are manageable. |
| Add flexible `fields?: string[]` / `intent?: string` request hints | 4 | 2 | 3 | P0/P1 | **A-** | Good dynamic interface, but too-open strings need descriptor discipline and validation. |
| Add generic content `extract` object for code/docs/config/manifests | 5 | 4 | 3 | P0 | **B+** | High value but implementation can sprawl; start with response metadata + simple scanners. |
| Add factual grouping/completeness to repo/local structure tools | 4 | 3 | 2 | P0/P1 | **B+** | Strong for layout questions; keep grouping supplemental so raw tree is not hidden. |
| Add `nextQueries` hints across tools | 4 | 3 | 3 | P1 | **B** | Useful for agents, but can become noisy or over-prescriptive if emitted too often. |
| Add search match classification (`matchRole`, docs/test/config/generated hints) | 4 | 3 | 3 | P1 | **B** | Helps evidence selection across content types; must be clearly heuristic. |
| Add package source/docs/repo hints | 3 | 2 | 2 | P2 | **B** | Low-risk workflow improvement; metadata can be stale so confidence is required. |

### Highest-confidence changes

1. **Add `mergedAt` to PR compact/TSV output.** Best impact-to-effort ratio.
2. **Add answer-readiness metadata.** Prevents meta-answers like “use returned values.”
3. **Add exact repo lookup/comparison response shape.** Fixes drift/current-value questions.
4. **Add completeness/truncation markers where not already explicit.** Prevents false complete lists.

### Changes to implement carefully

- **Open-ended `intent` / `extract.profile` strings**: flexible, but descriptors must explain accepted examples and fallback behavior.
- **Classification fields**: useful, but always mark confidence and never treat them as authoritative.
- **`nextQueries`**: emit only when high-confidence and small; otherwise it can steer agents incorrectly.

### Changes to avoid or defer

- Closed, benchmark-specific presets as the only interface.
- Language-specific extractor names in public schema unless wrapped in generic profiles.
- Heavy parsing that replaces raw evidence.
- Large default output expansions.

## Final recommendation

Build a **dynamic evidence layer** rather than rigid benchmark-specific modes. First verify missing data, then add response metadata and focused request fields. Keep extraction best-effort and narrow. This keeps Octocode token-efficient while making it much easier for agents to produce exact, grounded answers across code, docs, configs, packages, PRs, and repositories.
