# Core guidance and tool ratings

**Core guidance scores 8.9/10. Shared instructions score 9.0/10.** Each tool now names a next step and the evidence to carry into it. Eleven tools can supply a classifier source directly. A file reader can supply text or a whole PNG/JPEG. A clone supplies a checkout path for a subsequent read. The classifier guide also covers public HTTPS pages and named image assets. [Classify files and search results](../../../../../docs/OCTOCODE_CLASIFY.md)

## What the agent receives

| UTF-8 bytes | Before | After |
|---|---:|---:|
| Descriptions across 17 tools | 15,615 | 15,039 |
| Published MCP input schemas across 17 tools | 62,056 | 62,183 |
| Shared instructions with every tool enabled | 3,704 | 3,538 |
| Default MCP catalog and instructions | 53,544 | 53,525 |
| All 17 MCP tools and instructions | 84,310 | 83,694 |

Both catalog measurements use the same eight language-server labels. Required query fields lead, with the operation first when required. The schemas retain array batch limits, including the graph target maximum used by the viewer. The native enforcement contract matched the starting file byte for byte.

The twelve Markdown guides in `docs/` total 815 lines in the reviewed checkout, down from 1,445 at HEAD. The user guides retain examples, setup, and recovery instructions.

## Ratings out of ten

The first three columns assess whether an agent can choose the tool and its next step, form a valid request, and interpret the returned evidence. The core score gives equal weight to the mean of each column and the shared instructions. These are editorial judgments. The provisional tool ratings carry forward the [staged behavior audit](SCORECARD.md) and its verified dimensions. The package-tool rating returns to its preceding 8.4 after its two failing releases passed. This round exercised each tool and repeated interface parity checks. It did not repeat comparative timings or the full language-server matrix.

| Tool | Description | Input schema | Output reference | Tool readiness | Deciding detail |
|---|---:|---:|---:|---:|---|
| `structureSearch` | 9 | 9 | 9 | 9.0 | Listing → file read. A reply can still have another page when a row omits pagination. |
| `localSearch` | 9 | 9 | 9 | 9.0 | A hit supplies the path and line for a source read, or a set of hits for classification. |
| `localFetch` | 9 | 9 | 9 | 9.0 | Ranges select text. A PNG/JPEG can go whole to the classifier. |
| `astSearch` | 9 | 8.5 | 8 | 8.8 | Match → source or symbol lookup. The selected snippet schema states the parser requirement once. |
| `lspSearch` | 8.5 | 8.5 | 8 | 9.0 | A location leads to a source span. Syntax fallback still takes care to interpret. |
| `graphBuild` | 9.5 | 9 | 9 | 9.0 | A successful build returns the exact `data.graph` value to pin the next query. |
| `graphQuery` | 8.5 | 8.5 | 8 | 8.6 | Target limits support batching. Operations, evidence levels and nested continuations take more reading. |
| `clasify` | 8.5 | 8 | 9 | 8.4 | Search-hit scores live in `files[path][line]`. The text explains `units`, saved details and when a new page can bill. |
| `ghSearchRepo` | 9 | 9 | 9 | 8.8 | Pass owner and repository to tree or code search. |
| `ghSearchCode` | 9 | 8.5 | 9 | 8.7 | Read a candidate at the returned commit and use the tree for paths outside the index. |
| `ghStructure` | 9.5 | 9 | 9 | 9.0 | Pin a tree path to its commit and follow top-level paging before counting entries. |
| `ghGetFileContent` | 9.5 | 9 | 9 | 9.0 | A pinned file supports a precise read, history lookup or image classification. |
| `ghSearchHistory` | 9 | 8.5 | 8.5 | 8.6 | A number or commit selects the history item. The operation determines the date filter. |
| `ghGetHistoryItem` | 9 | 9 | 8.5 | 8.8 | Read a section, follow its continuation, or rank discussion entries. |
| `ghCloneRepo` | 9 | 9 | 8 | 9.0 | `location.path` leads to local searches. The receipt also identifies the checked-out commit. |
| `artifactSearch` | 9 | 9 | 9 | 8.4 | Resolve the exact release and use its source host and ref. Both non-GitHub Go pins retained their refs with consistent warnings. |
| `securityCheck` | 9 | 9 | 9 | 9.0 | The commit operation explains OSV egress. Advisory details keep severity and vector attributable to their source. |

## Checks

The rebuilt local CLI and a fresh stdio MCP server agreed on all 17 descriptions, resolved input schemas, output references and sampled rejection responses. Each tool received a real request. The classifier budget check used two files and made zero provider calls. A loopback provider received pixels from a standalone PNG and a JPEG named in text. Both requests returned their expected fixture answer. A public clone at commit `8fd127e76e8a4532116aeff73e52ff3a7bb8fa25` supported three subsequent local searches. The two Go modules returned registry refs beginning `6c97a165` and `73d21fdb` on the final build.

The graph fixture checked the handoff through an actual export. Two TypeScript files produced the import from `two.ts` to `one.ts`. Regression coverage includes paused and completed build receipts, a common witness source carried once per page, cycle-member continuations and counted impact lists.

The host binary SHA-256 was `696d44f030553b19ec8bc472c840b17bff9174c56935e8c0faace673329d75c1`. The verification applies to this local build.

All nine repository gates passed, including the host build, static checks, contract consistency, documentation and skills. The suites passed 1,915 core tests, 177 config tests, 9,363 native tests and 492 process tests. A separate sibling `yarn lint` run stopped on Prettier formatting in `packages/octocode-core/src/chromeContract.ts`. The changed research-tool files passed focused linting.
