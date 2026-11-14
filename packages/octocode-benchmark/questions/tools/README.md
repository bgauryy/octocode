# Per-tool capability questions

Each tool has specific, independently checkable questions. Run these alongside the shared
[CLI/MCP checks](../../CHECKS.md#every-tool-every-time), then rate the tool using the
[1–10 rubric](../../RATINGS.md). The question is the unit; do not replace a failed question with
an easier one or claim that an invalid request proves the positive path works.

| Tool | Questions | Relevant expert baseline |
|---|---|---|
| localSearch | [localSearch.md](localSearch.md) | rg with matching literal/regex, scope and count semantics |
| localFetch | [localFetch.md](localFetch.md) | Exact ranges; ast-grep-derived declaration range plus source read |
| structureSearch | [structureSearch.md](structureSearch.md) | fd/stat/wc with the same scope and ordering |
| astSearch | [astSearch.md](astSearch.md) | ast-grep structural/relational rules |
| lspSearch | [lspSearch.md](lspSearch.md) | Same server through direct LSP; rg only as a spelling baseline |
| graphBuild | [graphBuild.md](graphBuild.md) | Graphify code-only build, or an explicitly scoped source oracle |
| graphQuery | [graphQuery.md](graphQuery.md) | Graphify or equivalent projection of source-confirmed edges |
| clasify | [clasify.md](clasify.md) | Reading the same complete candidate evidence; count provider use |
| ghSearchRepo | [ghSearchRepo.md](ghSearchRepo.md) | gh search/repos with targeted fields |
| ghSearchCode | [ghSearchCode.md](ghSearchCode.md) | gh code search plus immutable source proof |
| ghStructure | [ghStructure.md](ghStructure.md) | gh API tree/refs with complete provider pagination |
| ghGetFileContent | [ghGetFileContent.md](ghGetFileContent.md) | Targeted raw source, exact ranges and explicit immutable cache |
| ghSearchHistory | [ghSearchHistory.md](ghSearchHistory.md) | gh search/history queries with matching scope |
| ghGetHistoryItem | [ghGetHistoryItem.md](ghGetHistoryItem.md) | gh item/compare sections and scoped patches |
| ghCloneRepo | [ghCloneRepo.md](ghCloneRepo.md) | git/gh clone setup plus equivalent repeated work |
| artifactSearch | [artifactSearch.md](artifactSearch.md) | Registry JSON or package CLI |
| securityCheck | [securityCheck.md](securityCheck.md) | Exact OSV/GitHub advisory queries |

The round's campaign holds fixture manifests, public commit IDs, package versions and expected
invariants. Freeze them before comparing answers. Keep question text free of answers and copyable
solution recipes. Dependent inputs may come from the preceding answer when the question explicitly
requires that workflow. Record operation/view coverage and every unverified question.

General checks are shared, not copied into 17 drifting recipes: current schema/help, valid/invalid
input, selected output, empty/partial/error distinctions, executable continuations, full page
reconstruction, cross-surface parity, first/repeat state, owned-process cleanup and honest cost
accounting. Large-language, ecosystem and provider coverage must be named explicitly.
