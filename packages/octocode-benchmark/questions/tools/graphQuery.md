# graphQuery questions

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
| GQ-01 | What does the fixture snapshot cover? Compare concise stats with detailed breakdown and provenance. | stats summary/details |
| GQ-02 | Find all candidates named `core`, select the exported symbol by returned identity, and inspect its node. Does ambiguity remain explicit? | find; candidates; node |
| GQ-03 | Enumerate the fixture's symbols with their file/declaration identities. | symbols |
| GQ-04 | What does entry import and who imports core, directly and transitively? Compare node, files and modules views against resolved module imports. | deps; dependents; depth; views |
| GQ-05 | Who calls exported core; what does entry call; what does a directed call-only walk reach? Exclude the shadow binding. | callers; callees; walk; confidence |
| GQ-06 | What directed call path connects entry to core? What path connects disconnected to core? Are unreachable nodes distinguished from unknown targets? | path; empty; identity |
| GQ-07 | Which files have the most incoming module imports? How does degree differ from PageRank? Keep the relation projection identical to the rival's. | hubs; degree/pageRank |
| GQ-08 | Which import cycles exist, what source witnesses close them, and what does runtime classification actually claim? | cycles; nested pagination |
| GQ-09 | What cycle/unreachable/unused-export hypotheses are reported with an explicit entry? Read the witnesses and avoid treating syntactic absence as deletion proof. | issues; detectors; minTier |
| GQ-10 | Which sites remain unresolved, especially the parameter-shadow call, and what diagnostic reasons limit absence claims? | unresolved; diagnostics; reason filters |
| GQ-11 | Enumerate all import/call witnesses in the fixture and every `want_bytes` call site in pinned Python source. Preserve repeated sites within one caller. | witnesses; sites versus edges |
| GQ-12 | What might depend on core when selected by node, changed file, or a pinned Git ref? How are these scopes distinguished? | impact selectors |
| GQ-13 | Following a disposable source edit, what does stale report before rebuild, and what structural changes does drift show against the original snapshot after rebuild? Restore source and check freshness again. | stale; drift; baseline |
| GQ-14 | Which files co-changed with pinned `encoding.py` in available Git history? Compare with exact read-only Git records and disclose history depth. | cochange; history scope |
| GQ-15 | Reconstruct symbols, witnesses, callers, cycles and issues with `limit:1` and the smallest supported response window. Follow every required response page before tool continuations; mixed rows and fresh/cross-surface cursors must preserve identity. | complete pagination |
| GQ-16 | How are an unknown operation, missing path endpoint, missing baseline, wrong limit, unsupported view, mutated cursor and unknown target repaired or refused? | near-miss/error contracts |

Compare source-confirmed selected edges and call sites first. Graphify's code-only CLI is the
rival where available; project module imports before comparing degrees, and label custom cycle
projection as custom work. An expert source incidence table is an oracle, not a timed end-user
competitor. Record each operation/view actually exercised and any unverified operation rather than
calling a small fixture complete language coverage.
