# OQL Internals — Transformer Architecture

**Contributor-only / internal documentation.**

You do NOT need any of this to use OQL. This document describes how OQL maps
canonical intent onto provider/tool backends. Skip it unless you are implementing
or debugging a transformer.

**OQL user docs:** [OCTOCODE_QUERY_LANGUAGE.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OCTOCODE_QUERY_LANGUAGE.md) · [OQL_LANGUAGE_REFERENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OQL_LANGUAGE_REFERENCE.md) · [OQL_RESULTS_AND_EVIDENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OQL_RESULTS_AND_EVIDENCE.md)

---

## What is a Transformer

OQL is one canonical language over many provider APIs. A transformer is the
boundary that translates between those two worlds:

```text
OQL query
  → transformer for target + source
  → provider/tool-specific query
  → provider/tool-specific response
  → transformer back to OQL rows, diagnostics, pagination, and evidence
```

Transformers keep the public OQL shape stable while GitHub, npm, local search,
LSP and future providers keep their own vocabulary. Agents
write OQL intent; transformers decide how that intent maps to the vendor or local
primitive.

**Current implementation note:** OQL has a transformer registry for active
source/target lanes. Each entry declares the adapter/backend contract; some
field-level lowering still lives inside the adapter module named by the registry.
The remaining work is to move all field-by-field lowering, loss diagnostics,
pagination mapping, and result-shape checks behind those registered transformer
contracts.

---

## Transformer Contract

Each transformer must be a first-class component with the same contract. Do not
hide provider-specific behavior inside the CLI parser, renderer, or ad hoc
adapter branches.

| Contract part | Required behavior |
|---|---|
| Capability declaration | State which OQL targets, sources, predicates, scope fields, fetch modes, params, and controls the backend can evaluate. |
| Input lowering | Map canonical OQL fields to backend fields, including provider naming differences such as `scope.language` → extension filters, provider language filters, backend language-family parameters, or include globs. |
| Exactness model | Mark each lowered field as exact, approximate, residual, routed, or unsupported. |
| Loss diagnostics | Emit `lossyTransform`, `vendorNoEquivalent`, `unsupportedVendorPredicate`, `requiresMaterialization`, or a more specific diagnostic before any meaning is dropped. |
| Pagination mapping | Normalize backend pagination into OQL `pagination` and `next.page`; preserve secondary domains such as per-file match pages, char windows, PR file pages, artifact scan offsets, and research packet pages. |
| Minification/content mapping | Map OQL `contentView` (`exact`, `compact`, `symbols`) to backend minify modes; report truncation/sanitization. |
| Output projection | Map backend data into stable OQL rows: `code`, `file`, `tree`, `content`, or `record` with the right `recordType`. |
| Continuation mapping | Attach executable `next.*` queries for exact reads, semantic proof, char ranges, match pages, materialization, graph proof, artifact scans, and structure/files follow-ups. |
| Error mapping | Convert backend errors and empty/provider-index ambiguity into typed OQL diagnostics with repair hints. |
| Explain trace | Show `oql.path → backend.path`, exactness, dropped fields, fallback routes, materialization, and result-shape expectations in `--explain`. |

**Two separate jobs — never confuse them:**

1. **Query transformation:** convert canonical OQL fields into the best provider query without dropping meaning silently.
2. **Result transformation:** convert provider output back into OQL rows, with stable `kind`, `recordType`, `path`, pagination, `next`, diagnostics, and evidence.

A query can be transformed correctly while the result shape changes underneath it;
that must become a `responseShapeMismatch` bug, not an empty research answer.

**Transformer diagnostics make lossy mappings visible:**

| Diagnostic | Meaning |
|---|---|
| `vendorNoEquivalent` | The OQL selector has no direct backend field. |
| `lossyTransform` | The backend query is valid but weaker than the OQL intent. |
| `unsupportedVendorPredicate` | The selected backend cannot evaluate the predicate; materialization or a different target is required. |
| `responseShapeMismatch` | The backing tool returned a shape the transformer did not understand. |

---

## Language Selector Logic

`scope.language` is canonical OQL intent, not a backend field. Transformers
decide how to project it. Never confuse provider dialect with OQL meaning.

| Selector | GitHub code search | GitHub repo search | Local ripgrep | Local file discovery | Structural AST | LSP |
|---|---|---|---|---|---|---|
| `ts` | `extension:"ts"` | `language:"TypeScript"` only for repo discovery | include `**/*.ts` or exact file-type filter | basename/include glob `*.ts` | include glob `*.ts` plus structural `lang` | file extension helps choose TS server; semantic op still needs file/uri |
| `tsx` | `extension:"tsx"` | `language:"TypeScript"` only for repo discovery | include `**/*.tsx` | basename/include glob `*.tsx` | include glob `*.tsx` plus structural `lang` | TS/TSX language server |
| `typescript` | `language:"TypeScript"` (lossy for file-complete proof — provider language coverage may not equal all known TS extensions) | `language:"TypeScript"` | TypeScript extension family | `*.ts`, `*.tsx`, `*.mts`, `*.cts` | structural include globs for known TS extensions | TS server; needs symbol/file anchors |
| unknown selector | pass only if backend accepts exactly, otherwise diagnose | pass only if provider accepts it | prefer explicit include globs or diagnose | prefer explicit globs or diagnose | diagnose unless parser language is known | diagnose or route to content/search first |

Rules:
- Exact extension selectors (`ts`, `.tsx`, `py`) stay exact when a backend supports extension filtering.
- Language-family selectors (`typescript`, `javascript`, `python`) can expand to known extensions locally.
- If a provider language filter cannot cover every known extension, emit `lossyTransform` or require materialization for proof.
- Multiple `scope.language` values must not be silently dropped. Either lower all values exactly, run multiple backend calls, or emit a blocking diagnostic.
- Unknown language selectors are never proof of absence.

---

## Supporting First-Class Components

The transformer architecture is more than one backend adapter per API. These
compiler pieces are also first-class transformer infrastructure:

| Component | Owns | Why agents need it |
|---|---|---|
| Canonicalizer + target-param validator | Sugar lowering, strict canonical OQL shape, ambiguity errors, common `params` validation | `--explain` must show the exact canonical query that ran. |
| Language selector transformer | `scope.language` projection into extension filters, provider language filters, local language-family parameters, include globs, structural globs, LSP file/server hints | One language intent, precise per-backend naming + loss diagnostics. |
| Capability planner + lossiness router | `PUSHDOWN`, `RESIDUAL`, `ROUTE`, `UNSUPPORTED`, backend choice, exactness, materialization requirements, provider approximation | Tell whether a result is proof, candidate, routed, or impossible. |
| Predicate compiler | `where` text/regex/structural/field/boolean into backend query knobs or local set algebra | Express conditions once without guessing ripgrep/AST/GitHub/file-search flags. |
| Result row mapper | Backend payloads into stable `code`, `file`, `tree`, `content`, `record` rows | One result shape even when providers return incompatible payloads. |
| Pagination mapper | Backend pages, char windows, per-file match pages, PR/file/comment pages, artifact scan offsets, research packet pages into OQL pagination/continuations | Know which result domain still has data and which continuation to run. |
| Evidence/envelope builder | `evidence.kind`, `answerReady`, `complete`, row `proofGrade`, diagnostics, provenance | Decide whether to answer, continue, or report candidate-only findings. |
| Continuation builder | `next.fetch`, `next.semantic`, `next.graph`, `next.page`, `next.matchPage`, `next.charRange`, `next.artifactStrings`, `next.structure`, `next.files` | Follow validated next steps instead of inventing paths, anchors, pages, or proof queries. |

---

## Transformer Inventory

Current active registry entries. Each owns the explain/provenance contract and
points at the backend plus adapter functions that perform the detailed
input/output mapping.

| Transformer | State | Must map |
|---|---|---|
| `github.code → ghSearchCode` | Active | `from.repo/owner/ref`, `scope.path`, `scope.language`, text/regex provider predicates, `params.extension/filename/match`, `limit/page`, provider-index diagnostics, path-level code rows, `next.fetch`, lossy language/path diagnostics. |
| `github.files → ghSearchCode` | Active | File-containing-term queries, `match:"file"`, deduped file rows, approximate provider semantics, materialization repair for exact file sets, pagination. |
| `github.content → ghGetFileContent` | Active | `fetch.content.contentView` → `minify`, range/context, match string/regex/case, char windows, branch/ref, content rows, `next.charRange`, truncation/sanitization diagnostics. |
| `github.structure → ghViewRepoStructure` | Active | `scope.path`, `fetch.tree.maxDepth`, sizes, repo/ref, tree rows, pagination, provider-empty diagnostics. |
| `github.semantics → ghCloneRepo + lspGetSemantics` | Active | Remote semantic operations via sparse materialization + local LSP; `params.uri`, operation `type`, symbol anchors, materialization provenance, semantic record rows. |
| `github.repositories → ghSearchRepos` | Active | `params.keywords`, `topicsToSearch`, `language`, owner, stars/size/updated/license/visibility/archived/sort/page, repository record rows, provider pagination, language selector mapping. |
| `github.pullRequests → ghHistoryResearch` | Active | PR list/detail, `prNumber`, state, author, labels, branch filters, keyword search, file/comment/commit pages, patch char windows, match scopes, PR record rows, secondary pagination and `next.*`. |
| `github.commits → ghHistoryResearch` | Active | path, branch/ref, since/until, includeDiff, commit/file pagination, commit rows, diff continuations, not-found/rate-limit diagnostics. |
| `github.diff.prPatch → ghHistoryResearch` | Active | PR patch lane (`prNumber`, `files`), diff rows, file-page/patch-page continuations, invalid-lane repair. |
| `github.diff.directFile → ghGetFileContent` | Active | Direct two-ref file lane (`baseRef`, `headRef`, `path`), content diff rows (real line diff), invalid-lane repair. |
| `github.materialize → ghCloneRepo/cache` | Active | Bounded repo/subtree clone, `scope.path/include/exclude`, force refresh, allow-full-repo guard, materialized checkpoint rows, `next.structure` and `next.files`. |
| `local.code.textRegex → localSearchCode/ripgrep` | Active | text/regex predicates, case/whole-word/multiline/dotall/fixed/PCRE2 flags, include/exclude/hidden/noIgnore, match windows, only-matching, counts, ranking/sort, per-file match paging, `matchTruncated`, code rows. |
| `local.code.structural → localSearchCode structural` | Active | Structural `pattern` or YAML `rule`, parser language, include globs, metavars/ranges, structural proof grade, parser/partial diagnostics, materialized/local-only capability. |
| `local.files → localFindFiles` | Active | Field predicates over path/basename/extension/size/modified/entryType, content-backed files routed through local search when needed, negative file sets, include/exclude/depth/hidden/noIgnore, local universe proof, file rows, pagination. |
| `local.content → localGetFileContent` | Active | exact/compact/symbol content views, ranges, context, match anchors, char windows, full content, content truncation, sanitized output, `next.charRange`. |
| `local.structure → localViewStructure` | Active | Tree depth, sizes, hidden, path scope, tree rows, pagination, structure continuations. |
| `local.semantics → lspGetSemantics` | Active | Operation `type`, uri/path, symbolName, lineHint, orderHint, workspaceRoot, depth, includeDeclaration, groupByFile, workspaceSymbol, supertypes/subtypes, diagnostics, LSP unavailable/capability diagnostics, semantic record rows, `next.fetch`. |
| `local.research → OQL research analyzer` | Active | goal/intent/facets/mode/maxFiles, full-scope summary, packet-domain pagination, graph capabilities, native graph facts, packet continuations, candidate evidence. |
| `local.graph → OQL graph analyzer + LSP proof` | Active | subject/relation/verdict/direction/proof/proofLimit/include flags, nodes/edges/facts/packets, missingProof, page-bounded LSP proof, proofStatus, `next.graph`, `next.fetch`, `next.semantic`. |
| `npm.packages → npmSearch` | Active | packageName vs keywords, mode lean/full, page, package rows, source repository hints, npm pagination/errors, follow-up repository/materialize continuations. |

### Registry Rules

- Every active target/source pair either has a transformer or a deliberate `unsupportedTarget` / `unsupportedPredicate` diagnostic with a repair.
- `search --scheme` and docs should be generated from transformer metadata where possible.
- New provider APIs are added by implementing a transformer, not by changing the public OQL language.
- Adapters may call backing tools, but they must not invent new OQL meaning that the transformer registry cannot explain.

---

## OQL Mapping Examples

| OQL intent | GitHub code search | GitHub repo/PR search | Local search | npm |
|---|---|---|---|---|
| `scope.language:"ts"` | `extension:"ts"` for file-level code search | `language:"TypeScript"` only when filtering repository language | TypeScript file/type filters | No direct field; emit diagnostic or defer to source-repo follow-up |
| `scope.language:"typescript"` | `language:"TypeScript"` or expand to TypeScript extensions when exact file types are needed | `language:"TypeScript"` | Expand to TypeScript extension family | No direct field |
| `where.kind:"text"` | `keywords` / provider text query | PR/repo keyword query when target supports it | ripgrep text search | Package-name or keyword search when target is `packages` |
| `where.kind:"structural"` | Not native; materialize first for local AST proof | Not native | Structural engine query | Not native |
| `fetch.content.contentView:"symbols"` | `ghGetFileContent` with symbol minification | Not a repo/PR-list field | `localGetFileContent` symbol view | Not native |
