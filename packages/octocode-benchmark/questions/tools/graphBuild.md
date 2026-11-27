# graphBuild questions

These are capability questions, not answer keys. Record exact source hashes, build identity,
schema, calls, failed attempts and every required continuation in the campaign. Apply
[shared checks](../../CHECKS.md), [ratings](../../RATINGS.md) and
[comparison accounting](../../COMPARISONS.md). Answers and independent incidence tables belong
in the campaign, never in this question file. Run valid and invalid calls on the pinned Node CLI
and a freshly started newline-framed MCP session. No all-language claim follows from this scope.

The frozen `semantic/inputs/fixture.json` for the 2026-11-14 tool-rating campaign contains the
source and SHA-256 of each file: JavaScript core/barrel/service/entry, shadows, an isolated
function, a two-module cycle, and TypeScript `Shape`, `Circle`, `NamedCircle`, a typed variable
and a Unicode-containing line. Materialize it in an isolated allowed scope. The second scope is
`pallets/itsdangerous` at `096c8d42545d3b68ea21a4f890fb2b2d8979c0bd`, limited to
`src/itsdangerous/*.py`; preserve its license and source hashes. Freeze questions before output.

| ID | Question and requested evidence | Coverage |
|---|---|---|
| GB-01 | Build the frozen fixture. What scope, snapshot identity, supported files, skipped files, linkage and limitations does the first receipt establish? Can its next query run unchanged? | normal build; receipt; identity |
| GB-02 | Build the unchanged scope again. Is the existing snapshot reused, and what proves reuse rather than merely a quicker process? | repeat; cache |
| GB-03 | Build the pinned Python package source. What selected declarations and call/import sites can be recovered from the resulting graph? | real source; independent oracle |
| GB-04 | After editing entry's direct dependency in a disposable fixture, does a build refresh the affected graph? Does `refresh:true` on unchanged source have its documented effect? Restore and verify hashes. | incremental/forced refresh; freshness |
| GB-05 | How do exclusion, hidden/ignored file switches and a parser glob alter selected scope? Are omitted and unsupported files disclosed? | scope controls; parserGlobs |
| GB-06 | Build an empty directory and a directory containing only unsupported text. Is absence distinguished from extraction failure? | empty/unsupported |
| GB-07 | How are a missing path, a file path, nonexistent scope, extra `operation`, wrong boolean and a two-query batch handled? Do refusal messages identify a runnable repair? | valid default; near miss; boundary |
| GB-08 | With the smallest supported response window, can the complete receipt and any nested evidence be reconstructed through unchanged continuations? If there is no pageable evidence, justify NA from the observed shape. | response pagination |

Compare selected source incidence and module-import projection before timing. Graphify code-only
builds, when available, use the same source and no assistant hooks/model backend. Graph sizes and
build times describe different graph models; they are not equivalent-payload compression ratios.
Use fresh graph namespaces for first-build samples and the same namespace for repeats. Do not
mistake a warm CLI runtime for snapshot reuse.
