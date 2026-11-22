# Public tool results and context ratings

All 17 tools were exercised directly. Two previous connected-service failures cleared. Search results now preserve evidence for the requested Git ref, and the compact advisory answer keeps its vector associated with the right record. The live repository search still withholds active repositories when the caller explicitly selects `archived`.

The [context inventory](CONTEXT.md) contains each exact description and schema size, the full shared instructions, instruction ratings, and the distinction between what a client loads at connection and what it can fetch later.

## Ratings

Ratings are provisional and scoped to the observations in the last column. Description is a separate editorial grade. The result score uses the benchmark weights for correctness (30), schema/input (15), output (15), pagination (10), workflow (10), efficiency (15), and freshness (5). The full efficiency comparison was unmeasured for every tool. A dash marks a dimension omitted from this round’s denominator. The live lost-evidence rule caps repository search at 4.0. Confidence is medium except for clone, where the positive path was unavailable. A score of 9 reflects strong evidence in the sample, and 10 requires broader coverage.

| Tool | Description | Schema | Correct | Output | Paging | Workflow | Fresh | Weight | Result / 10 | Fresh observed evidence |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `structureSearch` | 9 | 9 | 9 | 9 | 9 | 9 | — | 80% | **9.0** | Five paths arrived across three pages. The largest matched the filesystem size, and the path containing parentheses led directly to the selected source line. |
| `localSearch` | 9 | 9 | 9 | 9 | 9 | 9 | — | 80% | **9.0** | Both matching lines arrived in two pages. Only the clipped match offered a full-line read, which recovered the source bytes. |
| `localFetch` | 9 | 9 | 9 | 9 | 9 | 9 | — | 80% | **9.0** | Two pages reconstructed exactly 67,221 UTF-8 bytes on each interface. A 168-byte match window agreed with the independent byte slice. |
| `astSearch` | 8 | 8 | 9 | 9 | 9 | 9 | 9 | 85% | **8.8** | Two free calls matched ast-grep. File and snippet views produced the same 106 nodes across 15 pages. The selected schema leaves the choice of path or snippet plus parser implicit. Saved pages retained the 106-node tree after an edit and a fresh search found 114. |
| `lspSearch` | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 85% | **9.0** | Seven references over seven pages matched TypeScript. A global alias and a same-line shadow resolved to different declarations. An edit invalidated the old cursor and its restart returned eight references. |
| `graphBuild` | 9 | 9 | 9 | 9 | — | 9 | 9 | 75% | **9.0** | The TypeScript fixture yielded nine files, nine symbols, eight imports and seven linked calls out of eight. A repeat reused the snapshot and an edit produced a detectable change. The graph accepts one build per call. |
| `graphQuery` | 8 | 7 | 9 | 8 | 9 | 8 | 9 | 85% | **8.4** | A re-export query found two consumers. Its imports-only follow-up found one. Sixteen CLI/MCP pages recovered nine symbols and seven exact call-site windows. The operation-selected schema does not explain the narrower selector. |
| `clasify` | 7 | 7 | 9 | 8 | — | 9 | — | 70% | **8.4** | A fresh scout evaluated a public README for one billed request. Its 0.97 greeting score led to the pinned line “Hello World!”. A two-file request with an allowance of one returned zero calls and a confirmation. The summary and saved-detail wording disagrees with the available results. |
| `ghSearchRepo` | 8 | 7 | 4 | 4 | — | 4 | — | 70% | **4.0** | Asking for archived and defaultBranch withheld both active repositories in a two-row get. The live list also failed. The archived control succeeded and the provider independently confirmed all three statuses. The output contract requires true. |
| `ghSearchCode` | 9 | 8 | 9 | 9 | 9 | 8 | — | 80% | **8.7** | Five projected paths from Vite and GitHub CLI that were absent at the requested tags all carried atRef:false and a pinned commit. This passes the previously failing live check. |
| `ghStructure` | 9 | 9 | 9 | 9 | 9 | 9 | — | 80% | **9.0** | Five pages reconstructed the 20 Vite directory entries with the correct paths, types and sizes. The pinned file read matched the provider. |
| `ghGetFileContent` | 9 | 9 | 9 | 9 | 9 | — | — | 70% | **9.0** | Vite mergeConfig matched independently fetched source across six pages. The complete outline took 55 pages for 199 declaration headers and one import summary. |
| `ghSearchHistory` | 9 | 8 | 9 | 9 | 8 | 8 | — | 80% | **8.6** | Two README-history pages matched the expected commits and subjects. Two capped searches stopped repeating their narrowing links, and the regular next page advanced. |
| `ghGetHistoryItem` | 6 | 6 | 9 | 9 | 9 | 8 | — | 80% | **8.3** | Three pages reproduced a 1,213-character Vite PR patch. The commit, parent compare, body, head and reviews matched GitHub. The description still lists sections that the selected operation rejects. |
| `ghCloneRepo` | 9 | 8 | — | 8 | — | — | — | 30% | **8.0** | The opt-in server and CLI accepted their issued retry and rejected unsafe or invalid input. The network denied the public clone, leaving checkout and subsequent searches unverified in this round. |
| `artifactSearch` | 8 | 6 | 9 | 9 | 9 | 9 | — | 80% | **8.4** | TypeScript and braces resolved to their release commits. Three Babel lookup pages preserved 15 dependency names and an independent missing row. A synthetic single npm term returned fuzzy results despite the published matching guidance. |
| `securityCheck` | 9 | 9 | 9 | 9 | 9 | 9 | — | 80% | **9.0** | Ten pages preserved the affected and fixed braces versions and a withdrawn advisory. The CVE/GHSA retest kept distinct vectors in detail and omitted the unrelated vector in the compact result. |

## What the measurements establish

- A paid scout used one request with 356 input and 20 output tokens. Its generated evidence call read line 1 at `octocat/Hello-World@7fd1a60b01f91b314f59955a4e4d4e80d8edf11d`. A separate two-file preflight on the pinned binary reported zero requests sent.
- All ten commits between Vite v2.0.0 and v2.0.1 arrived in one selected compare. Independent ref lookups returned the same two pinned commits, and the tenth commit was the head release.
- The active GitHub projection failed for both `cli/cli` and `vitejs/vite`. An archived repository passed. The failure occurred when the output validator rejected an explicit false value, before the caller could read the requested row.
- The local host could start fresh MCP servers for every opt-in. Its network policy blocked successful external provider calls. Positive provider checks used the connected installation, whose binary fingerprint was unavailable. Clone was absent there. Local file, language and graph positive checks used the pinned CLI and fresh MCP.
- The package advisory checks compared version scope and source-specific records against OSV. [braces advisory](https://osv.dev/vulnerability/GHSA-grv7-fg5c-xmjg) · [CVE record](https://osv.dev/vulnerability/CVE-2022-25883)

## Improvements verified

While this round was running, the sibling working tree gained a boolean archive field. An independent validator built from the original core rejected the explicit active row and accepted the archived and compact rows. The rebuilt isolated validator accepted all three. A final live check still withheld the two active rows and returned the archived control. The audit patch adds a regression for all three shapes on top of the sibling fix.

The patch also clarifies the AST source choice, graph dependency defaults and grouping, legal history sections per operation, npm single-term relevance, and the classifier’s predicate scores, saved replay and nested paging. The shared instructions now use each tool’s own batch limit and distinguish charged inference from input-token counts and saved reads.

The combined patch applied to the adjacent working tree at `219c5f29df9c960a39acf6441a05c814c4f8b666` plus its local changes. An isolated copy passed **1,883 tests in 138 files**, typechecking, the build, public catalog generation and the existing core size-budget checks. The main project’s static checks and documentation verification passed. This round ran the core suite and focused public checks. It did not rerun the full native release gate.

Patch SHA-256 is `8c90b3c7862f71cff77d761695104e9f629d4fa95faa23418657d7010762726a`. The scores above describe the observed interfaces before deployment of the remaining changes.

## Workflow coverage

| Benchmark workflow | Fresh outcome |
|---|---|
| Find a remote symbol and read its body | An exact Vite file candidate and its commit led to the full `mergeConfig` declaration at lines 1555–1568. The source read identified the declaration despite a generic parameter between its name and opening parenthesis. |
| Locate a folder and read its outline | A five-page Vite directory walk returned 20 entries and led to a pinned read. A separate full outline of Vite source returned 199 headers and one import summary. The directory-to-outline sequence on a single file was covered in the preceding round. |
| Read a file’s history and a selected patch | Both README commits matched the independent history. The selected commit patch agreed with its parent-to-commit compare. |
| Read a merged PR, its files and reviews | The selected Vite PR’s head, changed file, body, reviews and three-part patch matched the provider. |
| Compare tagged releases | Both Vite tags independently resolved to the compare endpoints. The response contained ten commits, and its final commit was the release head. |
| Trace a package version to source | TypeScript and braces versions resolved to tags and immutable commits checked against their repository manifests. |
| Check a package version for advisories | Both version and purl selectors returned the expected affected and fixed scope. A withdrawn advisory remained reachable. |
| Clone and search repeatedly | The opt-in MCP and CLI both accepted their exact retries. The public network denied the initial clone. The three searches could not run. |
| Find a local symbol and read its source | Search and fetch recovered exact source bytes, and the syntax anchor led to the expected source span. |
| Resolve local callers and uses | Alias and shadow definitions differed as expected. References matched the independent TypeScript service and the property-use recovery reached its source line. |
| Find unreachable local code | An explicit entry in a three-file graph produced exactly one finding, `unused.ts`. The imported module remained reachable. |
| Scout unread sources and verify evidence | A one-request public README scout produced a pinned read of the line containing the greeting. |

## Tool classes and data boundaries

| Class | Tools | Data or action |
|---|---|---|
| Local reading | `structureSearch`, `localSearch`, `localFetch`, `astSearch`, `lspSearch` | Inspect selected workspace content. LSP uses an available language server. |
| Local graph | `graphBuild`, `graphQuery` | Build stores a derived snapshot. Query reads it and reports its scope. |
| Remote research | `ghSearchRepo`, `ghSearchCode`, `ghStructure`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem` | Send public repository coordinates, refs or search terms to GitHub. Structure can also materialize selected files locally. |
| Checkout | `ghCloneRepo` | Fetches a repository and writes a persistent checkout when permitted. |
| Registry and security | `artifactSearch`, `securityCheck` | Query a package registry or advisory provider with the supplied selectors. Security results retain advisory source and version scope. |
| Classification | `clasify` | A fresh evaluation sends selected evidence to the configured inference provider and can bill. The live test used a public greeting. |
