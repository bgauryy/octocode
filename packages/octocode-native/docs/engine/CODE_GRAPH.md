# Code graph architecture

The native `crates/engine` crate owns the reusable code graph model and algorithms. Runtime packages own path policy, cancellation, language-server lifecycle, pagination, redaction, and public tool response shaping.

## Models

One graph is queried and stored; the build has one intermediate:

- `BuiltGraph` (`runtime/src/tools/ast_graph/types.rs`): the linker's output for one scan, per-file facts plus the file graph (target file and edge kinds with their first line). Ingest projects it and drops it; nothing stores or queries it.
- `GraphTables` (`ast_graph/store/format.rs`): file, symbol and package nodes joined by compact relationship edges, with separate occurrence witnesses, unresolved sites, declaration evidence, diagnostics, file digests, components and entries. It is the snapshot `graph.bin` holds and the one model every `graph query` operation, `drift` included, reads. Current extraction emits `contains`, `imports`, `uses`, `calls`, `inherits` and `passes`. A `passes` relation observes a named value supplied as an argument; propagation and invocation remain unsupported.

The graph-facts scan (`engine/src/fact_scan/`) crosses the Rust package boundary as `GraphFactsMappedScan<T>`: `scan_walked_graph_facts` takes a `walk_graph_fact_files` walk, hands each parsed file to the caller's map as a borrowed `GraphFactsView`, and keeps only what the map returns. `FactsSource` picks full graph facts (`Graph`), validated reusable graph facts (`CachedGraph`), or the caller's cached declarations (`Declarations`).

## Ingestion flow

1. AST extraction produces typed declarations, imports, exports, calls, argument observations, modules, ranges, and source diagnostics (a code, a message and an optional line; facts schema v6) for every language in the [grammar registry](SUPPORTED_LANGUAGES_AND_FEATURES.md#language-matrix).
2. `build_graph` (`ast_graph/graph.rs`) scans a `GraphScan` (root, pruned directories, file bound, language globs, Cargo linking) and links each file's imports, re-exports, calls and paths into a `BuiltGraph`.
3. `tables::project` turns it into `GraphTables`, preserving occurrence witnesses before topology deduplication. After encoding, ingest checks recorded source identities and resolution inputs again before atomic publication. These checks detect observed mutations; they do not make a mutable working tree an atomic repository revision.

The graph is syntax evidence: `astSearch` and `graph` start no language server. Callers confirm identity with explicit `lspSearch` calls, which keeps syntax candidates distinct from semantic claims.

## Cycle engine

`ast_graph/algorithms.rs` holds the one cycle engine (`Cycles`): the dependency view of a file graph (data and module-tree edges removed), its strongly connected groups, and the groups its eager-load (runtime) edges close. `graph query cycles`, the `cycle` detector, `drift` and the health summary read it. The engine crate supplies the primitives (`engine/src/graph/algorithms.rs`): strongly connected components (iterative Tarjan) and the condensation.

## Agent graph tools (CLI and beta MCP)

`graphBuild` and `graphQuery` expose the same Rust store through the standard
`{queries:[...]}` envelope. Use `--json` for machine callers, including errors:

```sh
octocode --json graphBuild '{"queries":[{"path":".","keep":10}]}'
octocode --json graphQuery '{"queries":[{"operation":"stats","path":"."}]}'
octocode schema graphQuery --view query
```

MCP exposes these tools when `OCTOCODE_BETA=true` or home configuration enables
`local.beta`; restart the server after changing configuration. CLI availability
does not require beta. Discovery, admission and follow-up leads share the same
policy. `graphBuild` advertises `readOnlyHint:false` because it writes snapshots
and retention removes older ones; `graphQuery` advertises `readOnlyHint:true`.
Core owns their generated schemas, policy, annotations and descriptions; the Rust
runtime owns execution. Octocode Code registers native tools with these names,
reads the CLI schemas and invokes the installed binary with JSON on stdin. Its
`request` wrapper and `reason` field belong to the host, not the Rust CLI input.

`graphBuild` accepts one query per call; `graphQuery` accepts up to five.
Inspect `octocode schema graphBuild --view query` or the query schema above for
the contract embedded in the installed binary. Query alternatives reject
missing required inputs and options that do not apply to the chosen operation.

A build writes only under the current workspace's `.octocode/graph`, refusing
scopes or storage symlinks that escape it. Unchanged inputs and options reuse a
snapshot. The manifest's `analysisVersion` invalidates reuse after linking rules
change, independently of the binary format. The manifest also records the installed
registry identity and discovery extensions; provider, version or capability changes
invalidate reuse. An OS lock serializes build publication, `latest` and retention across
processes; cancellation releases the lock. `keep` defaults to three snapshots per
scope, with a maximum of 100 for retained baselines. Builds redact stored text and
return location, digest, counts, coverage, diagnostics and elapsed milliseconds.

Queries select the newest snapshot of exactly `path`, or an exact `graph` ID of
that scope. **Use the build's path even when supplying its ID**; omitting path
means the workspace root. The build's `related` stats call includes both fields.
Every successful query reports freshness, including added files and
linking inputs and whether the analysis version is current. A truncated scan or
older analysis cannot be reported fresh. Paging cursors pin the
snapshot ID; a pruned snapshot fails explicitly instead of switching to `latest`.
`responsePagination.next` and `next.*` continue evidence; `related` starts another
question. Traversal defaults
to high-confidence calls. Graph absence is not proof of unused code: verify symbol
identity and consequential callers with `lspSearch` and source reads. The `issues`
heuristics remain on the legacy graph CLI; the public query tool exposes the other
18 operations. No language server or LLM is required for snapshot creation.

### Choosing a query

| Operation | Inputs beyond scope | Result and defaults |
|---|---|---|
| `stats` | None | Counts, coverage, languages and diagnostic totals |
| `find` | Required `target` (a name, path or several words); optional `kind` | Ranked name/path search returning node IDs |
| `candidates` | Required `target`; optional `kind` | Exact reference candidates, using the same normalization and precedence as node resolution |
| `node`, `symbols` | Required `target`; optional `kind` | One node or its nested symbols |
| `deps`, `dependents` | Required `target`; optional edge filters, `direction`, `depth`, `kind` | Outgoing/incoming imports for files/packages, calls for symbols; one hop |
| `callers`, `callees` | Required `target`; same traversal options | Incoming/outgoing calls; one hop; file targets include their symbols and calls within the file. Recursive targets appear once when an edge reaches them. |
| `walk` | Required `target`; same traversal options | All edge kinds, outgoing, two hops |
| `path` | Required `target` and `to`; optional edge filters, `direction`, `kind` | Shortest candidate path; calls between two symbols, otherwise imports; no depth option. Condition compatibility is not assessed and execution is not observed. |
| `cycles` | Optional edge filters | Production cycles; imports by default |
| `hubs` | Optional edge filters, `kind`, `direction: in/out` | Production nodes ranked by incoming edge count by default |
| `diagnostics` | Optional `target` code or file prefix | Matching scan/link diagnostics |
| `witnesses` | Optional source `target`, destination `to`, edge filters | Occurrences supporting linked relations; a file target includes its symbols |
| `unresolved` | Optional source `target`, edge filters | Unlinked call or argument references and unsupported argument forms; a file target includes its symbols |
| `stale` | None | Source and linking-input changes since the snapshot |
| `impact` | At least one of `target`, `changed`, `since`; optional `kind`, `depth` | Combined change seeds, three hops by default |
| `drift` | Required `baseline` snapshot ID | Structural changes from baseline to selected snapshot; use matching scopes |

Edge filters are `edges` and `confidence`; confidence defaults to `high` and
affects only call edges, including in `witnesses`.
Unambiguous re-export bindings retain high confidence;
medium includes scoped or ambiguous candidates, and low includes global name
matches. Name matches without an import (same package, import scope, type
qualifier, receiver type, unique name) stay inside one language family:
JavaScript/TypeScript, C/C++/CUDA and Java/Scala each form one family, and
every other language is its own. Unique-name matches also need a name of at
least three characters. Java and C# never use them: a bare method call binds
the innermost enclosing class that declares or inherits the name (superclass
chain, then Java interface default methods; C# partial parts are one class),
then a static import or `using static`; an unresolved base or a root-object
member (`toString`, `ToString`) stops the search. Their types bind lexically,
then through a single-type import or `using` alias, the package directory (Java,
medium) or enclosing namespaces (C#), then on-demand imports or `using`
namespaces. Scala binds lexically and along the class linearization; a type an
unscanned wildcard import could supply stays medium. Python and Rust bare names
never use graph-wide matches: a name the module neither declares nor imports
stays unresolved. A Go bare name binds only a declaration of the caller's
package directory (high; medium from a `_test.go` caller, which may be the
external test package), never a method, and non-test files never see
`_test.go` files. Call traversals report their applied confidence. Stats rank called
symbols using high-confidence production edges and report `callConfidence`.
`cycles` and `hubs` exclude test edges. Import cycles include type imports;
`runtime: false` means that group contains no detected eager module-loading cycle. List operations
accept `limit` (default 50, maximum 1,000); `stats`, `node` and `path` do not.
Depth is bounded to 1–20 where supported. `impact` uses its own impact analysis
and does not accept edge or direction overrides.

An ambiguous reference returns a candidate preview and its complete total. Its
continuation resumes after the preview, preserving the failed reference, kind,
scope and snapshot. This applies to destination and change-seed references too.
`candidates` selects the first matching tier: stored key, case-sensitive name,
qualified symbol suffix, then file suffix. It trims whitespace and leading `./`;
continuations resolve against the recorded snapshot without consulting mutable
filesystem aliases. A reference no tier matches fails with up to five
`suggestions`: the first `find` rows for the same text.

`find` ranks every node whose name or key matches, in three tiers. `exact`: the
whole text equals a key or name, key first. `words`: the text and every name and
key split into identifier words (case changes and any separator: space, `_`,
`-`, `/`, `.`, `#`, `::`; letters and digits stay together), and each query word
must start a word of the name or key (each whole alphanumeric run also counts,
so `snapshotreader` matches `SnapshotReader`). The tier keeps only the nodes
matching the most query words: every word when any node has them all; otherwise
its rows carry `matchedWords`. Words score a name word equal (6) or started (5)
above a key word equal (3) or started (2), summed; fewer unmatched name words
rank first. `prefix`, `contains` and `path`: the whole text sits inside the name
at its start, inside the name, or inside the key. Ties rank by log(1 + incoming
calls and imports), times 0.1 for a name declared in more than five files or
starting with `_`, then by key. A single word therefore keeps the population of
a plain substring search: names and keys containing it.

`find`, `symbols` and `hubs` construct result rows only for the requested window;
`stats` constructs only each ranking's first five rows. Exact totals still need
full enumeration, and `find` sorts its whole population. Search still scans
nodes, symbols retain a traversal frontier, and hubs retain a full ranking
vector.
These operations check cancellation while scanning; cancellation does not return
a partial count as complete. No constant-memory or sublinear query claim follows.

What a query reads depends on its operation. `node`, `find` and `candidates` read
the lookup index, the record string table (`STRS`) and only the node and
declaration blocks their rows need. Topology operations read every record but
not occurrence evidence (`WITN`, `UNRS`) or the strings only that evidence uses
(`STRE`). The source catalog (`SRCE`) is read only when a result renders a
declaration or origin, and then only the origin blocks (`SRCO`) holding the
origins it renders; outside a full decode, each block's origin index is checked
structurally and each origin is decoded and validated when rendered.
`witnesses` and `unresolved` read every node, the record strings and the one
occurrence section they page; they count and filter every occurrence, then read
only the `STRE` blocks the rendered rows reference. Occurrences are checked
structurally on read; their full semantic validation runs when the snapshot is
published. Each read section or block is checksum-verified; corruption in a block
a query does not read is not detected by that query, and `validation` reports the
bytes and blocks a query read.

Traversals count every reachable result within the requested depth and filters.
They render only the requested page; continuations retain the full total and
reach the rest, including traversals with more than 200,000 nodes.

For a continuation, pass its entire `input` back to the named tool; do not add
query fields to a cursor request. If a cursor expires, rerun the original query
with its returned graph ID to preserve snapshot identity. For a new investigation, reuse a suitable
snapshot and check freshness before rebuilding. Choose a scope that contains
both ends of the relationships being investigated. Use `localSearch` or
`astSearch` for one-off lookups; a snapshot pays off across repeated questions.
Future parsers belong to the Rust [grammar registry](SUPPORTED_LANGUAGES_AND_FEATURES.md#language-matrix)
and core parser contract, so Octocode Code needs no grammar-specific change.

## Evidence and reuse

Topology answers deduplicate relationships; `witnesses` retains their individual
occurrences, including repeated calls and argument slots on one line. `unresolved` retains extracted
sites that the linker could not bind. Both operations page with `limit` and
`offset`, report the full matching total, and support executable continuation.
They report `evidence: "syntax"`; a linked call is a candidate, not proof that it
executed. A witness may represent a structural relationship and have no line.

Symbol nodes can carry `occurrenceId`, `sourceDigest`, `sourceRange`, `sourceOrigin`,
`parseAssessment` and `testIdentity`. Each observed range and digest refers to its
analysis unit's UTF-8 representation. Origins and the page's `sourceEvidence`
connect those coordinates to captured raw-file snapshots, including BOM/UTF-16
input and reparsed regions. [Source correspondence](SOURCE_CORRESPONDENCE.md)
owns the coordinate, branch, atomic-paging and limit rules. Queries preserve the
captured mapping after source changes or deletion. Structural projections without
an exact observed span explicitly return `null`. Each occurrence carries a
`derivation` rule and persisted version
combining the graph analysis and fact schema versions. Witness rules name the
edge family and actual resolver/projection detail; unresolved rules name the
edge family and failed-resolution reason. IDs identify occurrences within source and snapshot context; they
are not language-server symbol identities. Recovered declarations remain
inspectable but are excluded from certain call bindings. Test identities describe
recognized declarations, not test execution.

`linkage` and `linkageByLanguage` report `linked`, `eligible`, `excluded`,
`ratio` and `population: "syntax-call-sites"`. Eligible sites are linked sites
plus unresolved sites classified as plausible repository targets. Excluded sites
remain in unresolved evidence. The ratio is `null` when there are no eligible
sites; it measures linkage within this syntax population, not semantic recall.
Call-query `coverage.callExtraction` reports the grammar capability recorded in
the snapshot: `supported`, `unsupported` or `notAssessed`. Unsupported and unknown
capabilities carry an explicit warning. A language with no observed calls has its
own zero population and nullable ratio; it never borrows another language's
linkage. Supporting extraction does not imply semantic completeness.
Every `witnesses` and `unresolved` page includes this coverage, even when empty.
A source target selects its language's population across the snapshot; those
counts are not restricted to the selected source or matching evidence rows.
Without a source target, coverage includes every snapshot source language,
including languages that produced no rows. A destination filter does not narrow
that source scope. For multiple languages, `language` is empty, linkage is the
snapshot-wide extracted-site population, and the warning names supported,
unsupported and unassessed languages. `callExtraction` is supported or unsupported
only when all those languages share that recorded capability; mixed or unknown
capabilities report `notAssessed`.

Argument evidence uses the standalone recipient occurrence, even when a computed
recipient has no named call fact. `recipient.sourceRange` covers the full call-like
expression; the row's `sourceRange` covers the argument slot, and
`selectionByteRange` covers the selected name. `argument.groupIndex` and `index`
are zero based; a keyword label is retained where observed. A linked `passes`
edge points from the enclosing owner to the argument's binding candidate.

Exact declaration bindings are accepted directly. An observed import binding
resolves through that import and its export chain; it cannot fall back to an
unrelated same-file member. Name lookup is allowed only when the extractor
explicitly assessed its lexical scope. Bare names select the nearest visible
lexical parent; implicit member candidates require the grammar adapter to admit
that lookup and must belong to a lexical ancestor. Observed local or
parameter bindings outside the declaration inventory remain
`binding-unrepresented`, with `bindingSourceRange`; unassessed scope remains
`scope-not-assessed`. Recovered occurrences cannot establish bindings. These
barriers never fall back to a same-named global function. Unsupported slots and
whole groups retain their range, parse assessment and adapter `construct`;
unsupported groups have no invented slot index or target name. Argument
derivations identify `syntax-argument-reference:1` and the actual binding strategy
or unresolved reason.

`argumentCoverage` is always snapshot-scoped, including on filtered evidence
pages. Its independent `syntax-argument-slots` population records each language's
capability and reference forms. When every file of that language was extracted without recovery or a deadline,
counts include sites, groups, slots, direct references, non-reference values,
unsupported slots and unsupported groups. Slots partition into direct references,
non-reference values and unsupported slots; unsupported groups have no known slot
count. Unsupported or unassessed extraction has no fabricated counts. Build
receipts, `stats`, every evidence page and queries explicitly selecting `passes`
carry the persisted population. Argument observations do not change call linkage.

Changed trees reuse validated per-file extraction artifacts and rebuild all
bindings against the current file and configuration inventory. This handles
added, removed and renamed dependencies without relying on stale positive or
negative bindings. It does not implement dependency-based incremental resolution.
`extraction.parsedFiles` and `extraction.reusedFiles` report work for the build;
a whole-snapshot reuse reports zero parsed files. Interrupted builds can recover
committed extraction artifacts on retry. `extraction.resumedFiles` is the subset
of reused files recovered from an interrupted job; it does not mean linking or
projection resumed. Source digests are checked again before reuse.

The graph builder writes extraction progress under `.facts/jobs/<root digest>`.
The job records scan settings, source inventory, linking configuration and
parser/registry versions. Each artifact binds its source digest and complete
captured correspondence to the job. Artifact writes become durable before the
job advances. Extraction uses bounded input batches and restores one spilled
file at a time for linking. Secret-bearing artifacts stay in memory and must be
parsed again after interruption. `checkpointedFiles`, `spilledBytes` and
`memoryOnlyFiles` describe this work; `maxSpillBytes` and `maxMemoryOnlyBytes`
declare its terminal budgets (2 GiB of retained disk artifacts and 64 MiB of
memory-only artifacts per job). Atomic replacement can temporarily add one
artifact of at most 64 MiB; retry removes leftover temporary job files before
writing more. One current job is retained per canonical scope root,
including after completion for extraction reuse. Removed source artifacts are
pruned on retry; `force` or a changed extraction policy resets the job. This is
not a global disk budget across scopes and does not bound linking or projection
memory. A failed build leaves previously published snapshots available.

The disposable `.facts` cache used outside durable graph jobs namespaces artifacts by canonical root, parser
overrides, installed provider/capability identity, fact schema, runtime version and extraction version, and checks each
source path and digest before reuse. It has 128 replacement slots of at most
1 MiB each. Collisions, oversized facts, secret detection, invalid artifacts and
cache write failures produce cache misses; they do not discard graph evidence.
`force` bypasses stored extraction checkpoints and the disposable cache, while the engine may still reuse matching
facts in memory. Tests compare reused snapshots with a full binding and projection
rebuild after source, file-inventory and resolution-configuration changes.

## Persisted graphs (CLI)

`npx octocode graph ingest <path>` runs the `build_graph` linker. It projects the result into file, symbol, and package nodes with
`contains`, `imports`, `uses`, `calls`, and `inherits` edges, and publishes an immutable snapshot:

```text
<workspace>/.octocode/graph/
  .facts/                        # extraction cache and per-scope durable jobs
  latest                         # id of the newest snapshot
  <YYYYMMDDTHHMMSSZ>-<scope>-<root tag>/
    manifest.json                # root, scope, counts, link tallies, diagnostics,
                                 # linking-config digests (inputs), sha256
    graph.bin
```

The root tag is 6 hex digits of the scanned root, so two roots whose scopes
slug alike (`a-b`, `a/b`) never share an id, and pruning keeps snapshots per
root. Same-second publications use increasing numeric suffixes; retention orders
those suffixes numerically and never fills a pruned gap. Before any read, hash or
`git` call, a snapshot's manifest root must be canonical and inside the allowed roots.

Freshness has one owner (`ast_graph/freshness.rs`). It covers every source and the linking configs that change
resolution (tsconfig/jsconfig, `package.json`, `Cargo.toml`, `go.mod`, and the
like). Both ingest reuse and `graph query stale` use it: `stale` rescans the
scoped file list, so it reports `changed`, `missing`, `added` and `excluded`
sources and changed configs. A snapshot truncated at its file limit is never
reported fresh.

`graph.bin` (format v12) is a sectioned, varint-coded file:

- **Header:** magic `OCGRAPH\0`, format version, section count, and the
  SHA-256 of the section directory. This root is also recorded in the manifest.
- **Section table:** `{tag, offset, len, sha256}` entries. Ordinary sections hash
  their complete body; blocked sections hash a directory that commits to each
  record block. `NODE` and `DECL` use independently checked blocks of up to 256
  records.
- **`STRS`:** sorted unique strings, front-coded. Full canonical keys allow
  selected node blocks to decode independently; front coding shares repeated
  path prefixes.
- **`NODE` and `EDGE`:** varint columns. Edges are delta-coded by source, and
  kind, confidence and a test bit (every site of the edge is test code: a
  test file, or a Rust `#[cfg(test)]` item) share one byte.
- **`WITN`, `UNRS`, `DECL`:** linked occurrence witnesses, unresolved sites,
  and declaration source identity, parse assessment and optional test identity.
  Occurrences include representation byte spans, analysis-unit origins and derivation
  versions. Typed argument metadata references `STRS` and is validated separately
  from relationship details.
- **`SRCE`:** captured source snapshots, representations, analysis units and encoded
  origins. Numeric references avoid repeating descriptor IDs in every origin.
  The reader validates every origin, including those outside a query's selected
  page, and retains a compact offset/range/cost index. It reconstructs selected
  origins under the output budget. This required section stores correspondence
  and digests, not raw source text; each response fragment includes the complete
  descriptor closure for the origins it returns.
- **`LOOK`, `KEYX` / `NAMX`:** node lookup metadata and degree counts, plus key
  and name permutations. Full decode checks lookup metadata against node records
  and edge counts; selected node blocks are checked against their lookup entries.
- **`DIAG`, `FDIG`, `FCMP`, `ENTR`:** diagnostics (file, line, code,
  message, and the file node a diagnostic is about, when that is another
  file), per-file digests, components, and entrypoints.
- **`FSTM`** (optional): per-file size, mtime, ctime and inode, recorded only
  for files untouched for 2 seconds (`stamp::SourceStamp::SETTLE`) before the scan. An unchanged-tree re-ingest
  compares these stamps before hashing a source; a file without a stamp, or with
  a different one, is hashed.
- **Adjacency:** topology operations rebuild both CSR directions in O(V + E).
  Point operations obtain degree counts from the authenticated lookup metadata.

Encoding is deterministic. Full decode validates cross-references, evidence ownership,
line and byte ranges, typed declaration metadata and all stored blocks.
`node`, `find` and `candidates` select node/declaration blocks while retaining shared
string, lookup, source-catalog and freshness metadata. These operations avoid
decoding every node and occurrence, but their memory and I/O are not bounded by
the result-page size. Query `validation` reports checked blocks, snapshot bytes
read and decoded record counts. `accessedBlocks` leaves unread bodies unchecked;
`fullSnapshot` checks every stored block. Neither is a claim about semantic
analysis completeness. Snapshot read bytes exclude source-freshness reads.
Publication writes encoded sections to a staged file, avoiding an additional
whole-snapshot output buffer, and syncs the snapshot before publishing `latest`.
An unknown major version asks the user to re-ingest, and readers ignore unknown
section tags.

Call edges are syntax candidates. Each one is linked in one of these ways, in
this order:

1. A same-file declaration (with receiver-aware member preference).
2. An import binding, following named and star re-exports. Explicit exports take
   precedence over stars. A visited set terminates cycles without cutting long
   chains; multiple possible targets remain medium-confidence candidates.
3. A namespace or module qualifier (`ns.f`, `pkg.F`, `mod::f`): high when the
   bound module declares or exports exactly one target, the same for every
   language.
4. A unique name across the whole graph (`low` confidence).

Receiver calls bind the owning type before searching its `extends` chains.
Overrides stop a branch, repeated identities collapse, and cycles terminate.
`super` (Python `super()`) begins at the bases; `implements` does not supply a
method body. Rust wildcard imports follow their module bindings and re-exports;
explicit imports and declarations take precedence. Rust paths walk inline
`mod` blocks from the module that spells them (`nested::f`, `self::a::f`,
`super::f`, `crate::file_mod::inline::f`). A Rust method call prefers an
inherent `impl T` member over a trait impl's; a provided trait method `T` does
not override links through `impl Trait for T` at medium confidence. A Python
module's attributes are its top-level definitions and the names its imports
bind, so `from pkg import X` follows `pkg/__init__.py` re-exports; `from m import
*` binds names without a leading underscore (`__all__` is not read). Python
`@decorator` lines are `decorates` calls owned by the enclosing scope, and
imports under `if TYPE_CHECKING:` are type imports. A Go method is `Type.Method`:
`x.M()` reads the method set of `x`'s type across its package, then the
shallowest methods promoted through embedded fields or interfaces (`embeds`
heritage); an unresolved embedded type keeps a found method medium. Go import
aliases bind the package under the alias, dot imports bind like a wildcard and
blank imports bind nothing. A constructor-shaped receiver (`T(..).m()`,
`T::new(..).m()`, `NewT(..).m()`, a Rust unit struct value) types the call.
Multiple targets or unresolved branches keep observed candidates at medium
confidence.

CommonJS links the same way. A module-scope `require('./m')` binding (whole module,
destructured key or `.key`) counts as an import only when oxc semantics prove it
holds the loaded module; a guarded form such as `root.x || require('./m')` keeps
its file edge but links no call. A module exports only what unconditional
Program-level writes to `module.exports`, `exports` or `module.exports.k` prove.
A function, arrow or class written straight into such an export
(`exports.k = function () {}`, `module.exports = { k() {} }`,
`module.exports = function name() {}`) is a declaration named as callers write
it, and owns the calls inside it. An export inside an IIFE, a UMD wrapper or a
conditional proves nothing, so calls into it stay unresolved.

ES modules bind the way TypeScript does. A bare call never names a class or
object-literal member, which is never in lexical scope, so `mutate()` inside a
class that declares `mutate` reaches the imported `mutate`. A member call keeps
its receiver as written (`this.app.scene.mutate`, `get().run`). `ns.f()` through
`import * as ns` reaches that module's export `f`. `X.m()` through `import { X }`
or `import X` reaches the member `m` of the declaration `X` names: a class
static, an object-literal method or a namespace function. No same-named
declaration elsewhere is a candidate. A longer receiver path links only when the
syntax types it: a TS annotation, `new T()`, a class field, or a parameter
destructured under an interface, type alias or inline type literal of the same
file (`({ scene }: Props)`). Any other path stays unresolved.

C, C++ and CUDA link the way a compiler and linker see a translation unit. A
`.h` header holding syntax only C++ has (`namespace`, `template`, `class`,
`using namespace`, `extern "C++"`) parses as C++, and as CUDA when a
declaration names `__global__` or `__device__`; other headers stay C. A
prototype (`int f(void);`, an in-class `void draw();`) is not the function's
body: a call bound to it reaches the one definition with the same enclosing
namespaces and classes, an out-of-line `void Widget::draw() {}` included (several
definitions: the one whose file includes the prototype's header). A `static`
or anonymous-namespace function is a target only in its own file and the files
that include it. `ns::f()` and `::ns::f()` name the declarations whose enclosing
scopes end with (or equal) the written path, in the caller's file, directory or
included headers; a match only elsewhere is `low`. Template arguments
(`f<A>(x)`) are not part of the callee, `p->m()` is a member call like `p.m()`,
and a `<<<grid, block>>>` launch calls its kernel. An unparsable preprocessor
branch (`else` before `#ifdef`) is recovered as one statement; its sibling
statements keep their calls. Overloads of one scope all stay medium
candidates. Calls inside macro bodies, calls through function pointers and C
calls to assembly symbols are not linked.

Grammar adapters distinguish ordinary call lookup from named construction.
Named Java, C# and C++/CUDA construction can target type declarations only;
JavaScript constructors retain callable-name lookup. Same-file constructor
candidates must match the complete written container path and lexical visibility.
An explicit global `::Type` never falls back to a nested type. A unique exact
same-file candidate is high confidence; multiple candidates remain medium.
Cross-file names still depend on the linker's supported namespace/import rules;
an unresolved qualification is retained. Physical namespace occurrences remain
separate: unqualified visibility across reopened namespaces or equivalent compact
and nested namespace declarations is not inferred. Anonymous namespace containers
also stay distinct; implicit visibility outside them is not inferred. When matching
type declarations exist but this scope admission is unproven, the site reports
`constructor-binding-not-assessed` rather than claiming the type is absent.
Target-typed construction, constructor delegation and unsupported construction
forms retain their source occurrence with `constructor-target-unavailable`,
`constructor-binding-not-assessed` or `unsupported-constructor-form`. These
unassessed sites are excluded from the eligible linkage denominator, counted
separately and inspectable through `unresolved`. Parser recovery takes precedence
over binding admission. No argument becomes an invocation through this process.
Fact schema 12, analysis version 14 and binary format 14 invalidate earlier extraction,
binding and storage representations. Rebuild older snapshots with the current binary.

Every call edge keeps its `resolution` and `confidence`. `graph query` then
answers bounded, paged questions without rebuilding. The code is in
`crates/runtime/src/tools/ast_graph/store/`.

Ingest also classifies files by role, infers entrypoints, and reads manifests
into components and declared dependencies. That code is in `classify.rs`
and `store/workspace.rs`. Two more sections carry the results: `FCMP` maps each
file to its component and `ENTR` maps each entrypoint to the rule that found it.
Both are optional, so readers treat a missing section as empty. `uses` edges
link named imports to the exported declaration. `inherits` edges link a class,
interface, or impl to its base or trait.

`graph query issues` (`store/detect.rs`) runs the detectors on the loaded
snapshot. It needs no new dependencies:

- iterative Tarjan SCCs
- Eades–Lin–Smyth greedy feedback-arc cuts
- BFS witness cycles
- PageRank (damping 0.85, at most 50 iterations)
- articulation points with separated-side sizes
- Martin instability and abstractness
- Lakos CCD/NCCD over the SCC condensation, using bitset reachability

Finding ids hash the detector, the subject, and the identifying evidence, which
is what lets `--baseline` diff two snapshots. `--entry a,b` names the
entrypoints in place of the inferred ones (rule `explicit`) for every detector
that reads them.

`graph query drift --baseline <snapshot>` (`store/drift.rs`) diffs the
selected snapshot against another by root-relative key, so a checkout of the
base branch and the head compare: files added, removed and changed (content
digest), cycle groups added and resolved, file import pairs whose load turned
from module-load to deferred or back (`staticToDynamic`, `dynamicToStatic`),
and relations `(from, edge kind, to)` added and removed. A transition is not
also a removed and an added relation, and a moved line changes no relation.

`dependents <file>` with the default edges also lists the files that use the
file's exports through a re-exporting module (`edge: uses, via: reexport`).
TypeScript path aliases follow exact-match and longest-prefix precedence. Aliases
whose targets all point into `node_modules` count as external dependencies;
aliases with a project-source fallback still report unresolved internal links.
A bare JS/TS import that names a scanned file by path (`vs/base/common/x` for
`src/vs/base/common/x.ts`) but that no tsconfig `paths`/`baseUrl` or workspace
package links is no edge. Ingest records it as an `unlinked-path-import`
diagnostic naming that file, and `dependents <file>` lists those importers
under `summary.unlinkedPathImports` (`file`, `line`, `message`).
Every answer's `graph` block carries `partialReasons` (`maxFiles`,
`filesSkipped`) when the snapshot's scan was cut, and `reingest`, the widened
ingest, when `maxFiles` cut it.

Large inputs are bounded:

- **Over 1 MB:** a file is never parsed. It stays a file node with the
  `unparsed` role, and relative imports of it link through
  `unparsed-target` edges. The false unresolved diagnostics those imports would
  otherwise raise are dropped.
- **Bundles under the bound:** a file classified as bundled (minified,
  `sourceMappingURL`, webpack/parcel runtime, build output) keeps its file node
  and every extracted declaration, call site and argument observation. Roles can
  suppress noise-oriented detector findings but never remove research evidence.
  Compact and multiline source use the same binding and evidence paths.

### Known limits

- No LSP-enriched call identity: method calls whose receiver type is not
  locally evident stay unlinked rather than guessed. The answers that follow
  calls report syntax linkage counts and unresolved reasons so agents can inspect
  the gap; the ratio is not semantic recall.
- Per-file extraction can be reused, but binding and graph projection are rebuilt
  for the whole selected scope. There is no dependency-based resolution cache.
- `passes` covers named argument expressions admitted by each grammar adapter.
  It does not prove callable type, callback invocation, argument propagation,
  alias flow or execution. Unsupported forms remain explicit evidence; a missing
  relation is not proof that no callback exists.
- Point queries select node/declaration blocks; shared string, lookup and source
  metadata still scale with the snapshot. Other operations can require complete
  topology scans; occurrence bodies are skipped when the operation does not use
  them. `witnesses` and `unresolved` still decode every occurrence record of
  their section to count it exactly, but read evidence text and source origins
  one block at a time. Durable checkpoints resume extraction only; linking,
  projection and global graph algorithms remain in memory. Distributed builds
  and dependency-based incremental resolution are not implemented. Delivery
  cursors resume results, independently of extraction checkpoints.
- Cancellation is checked around snapshot loading and queries, and during evidence
  iteration. Decode and every global graph algorithm do not yet have fine-grained
  cancellation checkpoints.
- Snapshot and source identities do not model simultaneous compiler contexts or
  persistent semantic-provider identities. Source correspondence covers admitted
  decoding and installed region adapters; discovering arbitrary generated-file
  relationships or executing generators remains unsupported.
- C/C++ unreachable-file precision stays low: headers reach the build through
  `-I` roots and Kbuild/CMake rules, which ingest does not parse. Without a
  compilation database, C++ overloads are not resolved by argument types and a
  header's dialect is read from its own syntax, not the including unit.
- Rust call linking is limited by method chains and function return types;
  receiver facts do not propagate return types.

### Per-language limits

Each grammar is checked against its language's own name-resolution rules
(`link_tests/<language>.rs`). A form below stays unresolved, with its reason,
rather than linked by a name guess.

| Language | Not linked |
|---|---|
| TypeScript, TSX, JavaScript | Cross-file property types (`app.scene`), contextually typed parameters and generic callback types; members of `await import()`; UMD exports; guarded loaders (`root._ \|\| require(..)`); `module.exports = require('./impl')`; `require` inside a function body |
| Python | `cls()`; `__all__` is not read; a constructor call links to the class, not `__init__`; `self` inside a nested function |
| Go | an explicit type argument applied to one value (`F[T]` with a single argument, which the grammar reads as a type conversion); function values, type switches and method-chain return types. A `NewT()` result is typed `T` from the constructor's name, which is wrong when it returns an interface |
| Rust | calls inside macros; untyped closure parameters. A trait default method is matched to its implementing type by name |
| Java | method references (`Foo::bar`); untyped lambda parameters; `this(..)` and `super(..)` delegation (`constructor-binding-not-assessed`); record accessors and anonymous-class bodies |
| C# | extension methods and literal receivers; `: base(..)` initializers; a `global using` declared in another file |
| Scala | implicits, `given`, `summon` and extension methods; calls through the base type never reach an override |
| C, C++, CUDA | calls inside macros; calls through function pointers; overloads (every overload of the chosen type is a medium candidate); virtual dispatch (the static type's method only); only direct includes count as near for qualified calls |
| Assembly | a C call into an assembly symbol: assembly is its own link family |

### Open decisions

- **Query surface.** Fold the 20 ops into six primitives: `search` (`find`,
  `candidates`, `node`, `symbols`), `neighbors` (`deps`, `dependents`,
  `callers`, `callees`, `walk`), `path`, `evidence` (`witnesses`,
  `unresolved`, `diagnostics`), `analyze` (`stats`, `hubs`, `cycles`, `stale`,
  `drift`, `impact`), and a new `context` (search hits expanded into a
  budgeted subgraph). This is a contract cutover with no aliases.
- **Override edges.** Add an `overrides` edge kind for Java, Scala and C++
  virtual calls, or keep override dispatch in `lspSearch`.
- **C# `global using`.** Linking it needs a global marker on the shared
  `Import` record.
- **Java `package` declarations.** Extracting them would make same-package
  links `high` (today the package is inferred from the directory, `medium`)
  and replace the `low` unique-name fallback for split source roots.
- **C++ overloads.** Narrow a call by argument count, or leave an overloaded
  name unlinked. Today all overloads link at `medium`; CUTLASS `to_string`
  fans out to about 70.
- **C/C++ header dialect.** Keep reading a header's dialect from its own syntax,
  or follow scip-clang and take the language of the files that include it.
  That needs the include graph before parsing.
- **Transitive includes.** Count transitively included headers as near for
  qualified C/C++ calls. Recall rises, at the cost of a wider candidate pool.

Symbols carry a test flag: symbols in test files, symbols inside Rust test
modules (`mod tests`, `mod proptests`, …), and pytest `test_*` functions and
`Test*` classes. `impact.testFunctions` lists the flagged symbols a change
reaches.

CLI graph queries page affected rows with `--limit` and `--offset`. Large
`impact` summary arrays and `issues` baseline arrays show 100 entries in the
initial response and provide one executable `nextLists` command per array.
For example, `--list summary.testsToRun` returns the complete test list as
ordinary paged `results`; follow its `next` command until it ends. A cycle
larger than 50 nodes includes `nextNodes`, which pages every member through
`--list cycleNodes`. Generated continuations retain an explicit `--workspace`
and pin the graph snapshot ID.

Every build and snapshot bound lives in one place
(`crates/runtime/src/tools/ast_graph/limits.rs`). Snapshot reads are bounded at
512 MiB. Decode permits at most 64 sections, 5 million items per table,
2 million edges, and 512 MiB of expanded string content. A build is refused
with `scopeTooBroad` and narrower-scope hints as soon as its snapshot can no
longer publish:

- **Scan:** the running source-correspondence charge (descriptor allocations,
  the origin index, reconstruction cost and retained encoded bytes) and a tally
  of the nodes and `contains` edges each file projects.
- **Link:** the same tally plus each distinct file edge. One bound counts both
  the build's edges and the snapshot's EDGE rows, so there is no degraded
  edge-capped graph.
- **Projection:** every table's row count, before secret redaction.
- **Encoding:** expanded string bytes and the `graph.bin` size.

A build exactly at a bound publishes. Reads also reject snapshots beyond their
bounds. These are input and table limits, not a hard process-memory
budget; decoded tables, witnesses and algorithm state also consume memory.

## Evaluation

Correctness gates cover typed-fact fidelity, occurrence preservation and paging,
unresolved-site retention, malformed evidence rejection, clean/reused equivalence
after dependency and configuration changes, mixed-language capability reporting,
publication limits, snapshot retention, structural diff
identity, reverse-graph invariants, SCC partitioning, and native response parity. Run:

```bash
cargo test --manifest-path packages/octocode-native/Cargo.toml -p octocode-engine --no-default-features graph::
cargo test --manifest-path packages/octocode-native/Cargo.toml -p octocode-engine --no-default-features --test graph_benchmark frozen_graph_correctness_receipt_is_stable -- --exact
cargo test --manifest-path packages/octocode-native/Cargo.toml -p octocode-native tools::ast_graph
```

The language battery ingests one pinned repository per grammar
(`packages/octocode-benchmark/local/repos`) and compares the files `callers`
returns for a probe function with the files ripgrep finds calling it. Last run,
2026-10-29:

| Language | Repository (dir) | Probe | Recall | Precision |
|---|---|---|---|---|
| C | redis (`src`) | `createStringObject` | 1.00 | 1.00 |
| Rust | tokio | `poll_proceed` | 1.00 | 1.00 |
| C# | Newtonsoft.Json | `ArgumentNotNull` | 0.96 | 1.00 |
| Java | guava (`guava/src`) | `checkNotNull` | 0.95 | 1.00 |
| Python | django | `force_str` | 0.95 | 1.00 |
| CUDA | cutlass (`tools/library`) | `make_gemm_real_canonical_layouts` | 0.95 | 1.00 |
| Go | prometheus | `FromStrings` | 0.94 | 1.00 |
| TypeScript | TypeScript (`packages`) | `areTestsFiltered` | 0.91 | 1.00 |
| Assembly + C | libjpeg-turbo | `jpeg_std_error` | 0.89 | 0.89 |
| Scala | cats (`core`) | `tailRecM` | 0.69 | 1.00 |
| C++ | nlohmann/json | `skip_whitespace` | 0.67 | 1.00 |
| TSX | excalidraw (`packages`) | `mutateElement` | 0.56 | 1.00 |
| JavaScript | lodash | `isObject` | 0.25 | 1.00 |

The ripgrep oracle matches text, not bindings:

- **Assembly + C:** the one "extra" file (`fuzz/decompress_libjpeg.cc`) calls
  the function through an `extern "C"` include, so it is a real caller.
- **C++:** the miss is an amalgamated header calling its own copy of the function.
- **JavaScript:** six of the twelve oracle files are vendored libraries with
  their own `isObject`, or a benchmark string. The rest reach lodash through a
  UMD export.
- **TSX:** the misses need receiver types from other files.

Timing is a criterion benchmark, not a CI threshold. It builds and queries the same frozen graph (`OCTOCODE_GRAPH_BENCH_FILES`, default 20,000 files, and `OCTOCODE_GRAPH_BENCH_FANOUT`, default 4):

```bash
cargo bench --manifest-path packages/octocode-native/Cargo.toml -p octocode-engine --bench graph
```

Measure any proposed graph library or incremental parsing against the same correctness test and benchmark before adopting it.
