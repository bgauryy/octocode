# OQL Research Graph Flow

This is the agent algorithm for repo research questions such as:

- "What looks dead?"
- "Why?"
- "What keeps it alive?"
- "Is that keeper itself dead?"
- "What proof is missing?"
- "What exact file/line should I inspect next?"
- "Is this safe to delete?"

The flow is graph thinking, not a rigid command recipe. Each step adds facts,
edges, or proof strength.

This doc owns the *algorithm*. Field, target, and `params` syntax is the
language reference: `OCTOCODE_QUERY_LANGUAGE.md`.

## The Algorithm

```text
1. Structure
   ls / structure search / file search
   -> repo shape, package roots, manifests, source dirs, tests, generated dirs

2. Discovery
   ripgrep text grep / regex grep / file-content search / pattern search
   -> cheap anchors, candidate symbols, import strings, dynamic usages, file sets

3. AST inventory
   native graph facts / AST grep / structural search / document symbols
   -> declarations, imports, exports, calls, classes, functions, containers

4. LSP proof
   graph proof:lsp / references / definition / typeDefinition / implementation / callers / callees
   -> semantic identity, real references, call graph, keeper proof

5. Graph reasoning
   nodes + edges + entrypoints
   -> reachability, internal-only exports, transitive-dead pruning

6. OQL packet
   target:"research" packet or target:"graph" relationship view
   why + retainedBy + missingProof + risk + next
   -> answer the agent can inspect, page, and prove further
```

No single step is enough. Safe answers come from the graph assembled across
steps.

## Question Map

| Question | OQL field / result |
|---|---|
| "What looks dead?" | `target:"research"`, `intent:"reachability"` |
| "Why?" | `packets[].why`, `provenance` |
| "What keeps it alive?" | `target:"graph"`, `subject`, `direction:"incoming"`, optional `proof:"lsp"` |
| "Is that keeper itself dead?" | `target:"graph"` filtered by keeper, then graph/LSP proof |
| "What proof is missing?" | `missingProof`, `diagnostics` |
| "What file/line next?" | `next.fetch`, `line`, `range` |
| "Is this safe to delete?" | only after proof-grade evidence and project risks are clear |

## Search Surface Map

Use the cheapest surface that can answer the current sub-question, then promote
important candidates to AST or LSP proof.

| Surface | OQL target | Quick command | Use for | Evidence role |
|---|---|---|---|---|
| Structure search | `structure` | `ls` | Directory shape, package roots, source/test/generated areas | Corpus boundary |
| File search | `files` | `find` | Basename, extension, path, metadata, files containing or missing text | File-set candidate |
| Content grep | `code` with `text` | `grep` | Literal mentions, import strings, config keys, dynamic strings | Discovery anchor |
| Pattern grep | `code` with `regex` | `grep` | Broad captures, repeated names, route/env/specifier extraction | Discovery anchor |
| AST grep | `code` with `structural` | `grep --pattern` / `grep --rule` | Declarations, imports, exports, calls, containment, code shape | Structural proof |
| Native graph facts | `graph` / `research` | `search` | JS/TS declaration/import/export/call inventory from Rust/OXC | AST fact base |
| Exact content read | `content` | `cat` | Inspect the exact declaration, match range, or keeper line | Human/proof slice |
| LSP semantics | `semantics` | `lsp` | References, definitions, type definitions, implementations, callers, callees | Semantic proof |
| Relationship graph | `graph` | `search` | Nodes, edges, retained-by chains, missing proof, exact packet continuations | Candidate graph view |

## Evidence Tiers

| Tier | Foundation | What it can prove | What it cannot prove alone |
|---|---|---|---|
| Tier 1 | Structure + AST + LSP + graph | Strong symbol proof and bounded retained-by answers when missing proof is closed | Framework or runtime behavior not modeled by policy |
| Tier 2 | Structure + AST + graph | Structural candidates and import/export relations | Semantic references, overloads, type-only identity |
| Tier 3 | Structure + ripgrep/file search | Discovery, anchors, suspicious patterns, dynamic clues | Safe deletion or semantic absence |

Rule: ripgrep/text facts can create candidates and `missingProof`; they must not
produce deletion-grade proof.

## Step 1: Structure

Start by bounding the universe. Agents should understand the repo before asking
semantic questions.

Use:

- OQL `target:"structure"` for directory shape.
- OQL `target:"files"` for file universe, file predicates, and file-set search.
- CLI `ls` / `find` as quick aliases.

Collect:

- package roots and workspace boundaries;
- `package.json`, `tsconfig*`, framework configs, test configs, build configs;
- source, test, generated, dist, fixture, example, and script directories;
- likely entrypoint files: `src/index.*`, `main`, `bin`, route files, CLIs;
- file counts and pagination state.

Output facts:

```text
file node
directory node
manifest node
config node
entrypoint candidate
ignored/generated/test classification
```

Advanced structure techniques:

- Use shallow tree first, then deepen only interesting dirs.
- Use file predicates for extension, basename, path, modified time, entry type,
  and contains/does-not-contain checks.
- Page broad trees; never assume page 1 is complete.
- Separate source files from tests, fixtures, generated output, examples, and
  build artifacts before judging dead code.
- Treat missing local/materialized corpus as `missingProof`, not absence.

## Step 2: Discovery With Ripgrep And File Search

Use ripgrep to find anchors cheaply. This step narrows where AST/LSP should work.

Use:

- OQL `target:"code"` with `where.kind:"text"` or `where.kind:"regex"`.
- OQL `target:"files"` with file predicates and content predicates.
- OQL `target:"content"` only after an anchor exists, to read exact ranges.
- CLI `grep` / `find` as quick aliases.

Good discovery questions:

- Where is this name mentioned?
- Which files import this package?
- Which files contain dynamic import strings?
- Which files do not contain a required registration?
- Which config files mention this path or basename?
- Which patterns appear many times?

Advanced ripgrep techniques:

- `view:"discovery"` or files-only mode for first pass.
- `target:"files"` with content predicates to return files, not match rows.
- `onlyMatching` to enumerate values from minified or dense files.
- `countUnique` to rank repeated import specifiers, route names, env names, or
  function names.
- `matchWindow` for dynamic import strings and config references.
- CLI `grep --files-without-match` or OQL negative predicates for missing
  registration checks.
- `include`, `exclude`, `excludeDir`, language filters, and path scopes before
  reading content.
- Regex for broad candidate capture; structural AST for proof of code shape.

Output facts:

```text
text mention
pattern match
candidate import string
candidate dynamic edge
candidate config edge
file-set predicate
```

Discovery facts are useful, but they are not proof. Promote them through AST or
LSP before deciding.

## Step 3: AST Inventory

AST inventory turns files into structured facts.

Use:

- OQL `target:"code"` with `where.kind:"structural"` for AST grep.
- OQL `target:"graph"` or `target:"research"` to reuse native JS/TS graph
  facts extracted by the Rust engine.
- OQL `target:"semantics"` with `params.type:"documentSymbols"` when LSP is
  available.
- Engine structural grammars and OXC JS/TS support under the hood.

Collect:

- declarations;
- imports and reexports;
- exports;
- calls;
- class/function/method containers;
- extends/implements/type-use relations when available;
- symbol line/range anchors for LSP.

Output facts:

```text
symbol node
defines edge
imports edge
exports edge
calls edge
container edge
type-use edge
```

AST facts prove code shape. They do not always prove semantic identity across
files; LSP does that.

## Step 4: LSP Proof

Use LSP to upgrade candidate facts into semantic proof.

Use:

- OQL `target:"graph"` with `params.proof:"lsp"` to prove the current page of
  symbol packets inside the graph flow.
- `documentSymbols` to anchor symbols when AST inventory is incomplete.
- `references` with `includeDeclaration:false` to prove usage.
- `definition` / `typeDefinition` / `implementation` to resolve identity.
- `callers` / `callees` / `callHierarchy` to prove function flow.

Classify every reference:

```text
same-file
same-package internal
external package/workspace
test-only
generated
type-only
dynamic/string-only
entrypoint-reachable
dead-retained
```

Output facts:

```text
semantic symbol identity
reference edge
definition edge
implementation edge
caller edge
callee edge
proof diagnostic
```

If LSP is unavailable, partial, or paginated, keep the packet candidate-grade and
emit `missingProof`.

Graph proof is intentionally page-bounded. Use `proofLimit` to cap the number
of current-page symbols to prove, then follow `next.page` for the next packet
page.

Interpreting reference counts:

- LSP `totalReferences:0` is proof the server found no references in the
  workspace it can see.
- References only from symbols that are themselves `totalReferences:0` are
  transitive-dead evidence.
- References from tests, scripts, generated files, config, or dynamic loaders
  may still retain code, depending on the project.
- File and dependency deletion additionally need entrypoint and framework
  awareness: OQL gathers the evidence, the verdict needs project context.

## Step 5: Graph Reasoning

The graph joins structure, discovery, AST, LSP, manifests, and policy.

Core graph:

```text
entrypoint -> file -> import/export -> symbol -> references/calls -> retainers
manifest -> dependency -> usage
config -> generated/runtime/framework edge
```

Algorithms:

1. Mark entrypoint-reachable files and symbols.
2. Build reverse retainers for every file and symbol.
3. Classify references as same-file, internal, external, test-only, type-only,
   dynamic, or unresolved.
4. Collapse cycles with strongly connected components.
5. Mark internal-only exports when refs never cross the public boundary.
6. Mark transitive-dead when every retainer is dead or non-public.
7. Keep unresolved dynamic/framework edges as `missingProof`, not proof of
   absence.

Current implementation status:

| Capability | Status | Agent interpretation |
|---|---|---|
| Native JS/TS graph facts | Implemented | `research` / `graph` expose native AST declaration/import/export/call coverage through `nativeGraphSummary`; detailed `research` can include `graphFacts`. |
| Research packets | Implemented | `target:"research"` returns paged packets with `why`, `retainedBy`, `missingProof`, `risk`, and `next.*`. Treat as candidate evidence. |
| Relationship view | Implemented | `target:"graph"` filters the packet domain by subject, relation, direction, and verdict, then returns nodes, edges, facts, missing proof, and optional packets. |
| Page-bounded LSP proof | Implemented | `params.proof:"lsp"` or `mode:"prove"` attaches `proof.lsp` to current-page symbol packets up to `proofLimit`. |
| Entrypoint reachability | Partial | Current package/manifest reachability is useful for triage but not framework-complete. File deletion remains candidate-grade. |
| Retained-by chains | Partial | Retainers are currently a mix of AST facts and token/ripgrep references, upgraded only for current-page symbol packets when LSP proof runs. |
| Transitive-dead pruning | Partial | Candidate transitive-dead rows exist, but LSP-proven references are not yet recursively interpreted as "used only by dead code." |
| Internal-only export detection | Pending | Same-file/internal references are not yet a first-class public-boundary proof. |
| SCC/cycle collapse | Pending | The intended graph algorithm is documented here, but current packets do not yet expose SCC groups. |
| Framework/package policy | Pending | Dynamic routes, framework conventions, scripts, generated files, and config globs must remain `missingProof` unless separately checked. |

Key verdicts:

| Verdict | Meaning |
|---|---|
| `reachable` | Retained by an entrypoint or proven external consumer. |
| `candidate-dead` | No proof of reachability yet. Needs LSP/AST/policy proof. |
| `internal-only-export` | Exported, but only same-file/internal refs keep it alive. |
| `transitive-dead` | Referenced only by code that is itself dead. |
| `candidate-unused-file` | No graph path from entrypoints yet. Needs framework/dynamic proof. |
| `candidate-unused-dependency` | Declared package has no proven usage yet. |
| `unknown` | Evidence conflicts or proof is incomplete. |

## Step 6: OQL Packet

Packets and graph rows are the agent-facing graph surface:

- `target:"research"` returns the candidate packet list and repo-level
  research summary.
- `target:"graph"` returns relationship nodes, edges, facts, missing proof, and
  optional paged packets filtered by subject, relation, direction, and verdict.

They should answer:

```text
what is the subject?
what is the verdict?
why?
what keeps it alive?
is that keeper itself alive?
what proof is missing?
what exact query/file/line next?
what is the deletion risk?
```

Packet shape:

```ts
type ResearchPacket = {
  subject: { kind: string; name?: string; uri: string; range?: Range };
  verdict: string;
  proofStatus: string;
  proof?: { lsp?: { status: string; totalReferences?: number; files: string[] } };
  why: RelationFact[];
  retainedBy: RelationFact[];
  missingProof: MissingProof[];
  risk: { deleteRisk: "low" | "medium" | "high" | "unknown"; reason: string };
  next: Record<string, OqlContinuation>;
};
```

Graph query shape:

```json
{
  "target": "graph",
  "from": { "kind": "local", "path": "." },
  "params": {
    "intent": "reachability",
    "subject": "symbolOrFileName",
    "verdict": ["candidate-dead", "transitive-dead"],
    "relation": "references",
    "direction": "incoming",
    "proof": "lsp",
    "proofLimit": 5,
    "includePackets": true
  },
  "page": 1,
  "itemsPerPage": 25
}
```

Safe-deletion rule:

```text
candidate != safe to delete
text-only != safe to delete
AST-only != safe to delete unless the project has no LSP path and policy says so
LSP proof + graph reachability + closed missingProof = deletion-grade
```

## Engine Grammar Coverage

The engine grammar registry should feed capability tiers. Unsupported facts must
be diagnostics, not silent absence.

| Family | Extensions |
|---|---|
| JavaScript / TypeScript | `ts`, `tsx`, `mts`, `cts`, `js`, `jsx`, `mjs`, `cjs` |
| Python | `py`, `pyi` |
| Go | `go` |
| Rust | `rs` |
| JVM / CLR | `java`, `kt`, `kts`, `scala`, `sc`, `sbt`, `cs` |
| C family | `c`, `h`, `cpp`, `hpp`, `cc`, `cxx`, `hh`, `hxx` |
| Shell | `sh`, `bash`, `zsh` |
| Ruby / PHP | `rb`, `rake`, `gemspec`, `ru`, `php` |
| BEAM | `ex`, `exs`, `erl`, `hrl` |
| Infra / data / protocols | `json`, `jsonc`, `yaml`, `yml`, `toml`, `tf`, `hcl`, `tfvars`, `sql`, `proto` |
| Markup / style | `html`, `htm`, `css`, `scss`, `less` |
| Other native grammars | `lua`, `ml`, `mli`, `zig`, `r`, `jl`, `swift` |

Coverage fields to read before claiming absence:

| Field | Use |
|---|---|
| `graphCapabilities.graphFactExtensions` | Extensions that can enter native graph inventory. |
| `graphCapabilities.sourceFilesByLanguage` | Languages the current corpus contributed. |
| `graphCapabilities.graphFilesByLanguage` | Languages that actually emitted graph facts. |
| `graphCapabilities.missingGraphFacts` | Source files that entered but produced no graph facts. |
| `nativeGraphSummary` | Counts of files, declarations, imports, exports, calls, and edges from AST. |

Missing capability is not proof of absence.

## Responsibility Split

Rust engine:

- parse files;
- extract AST facts;
- normalize symbols and relations;
- connect graph nodes and edges;
- run deterministic graph algorithms;
- return paginated facts, diagnostics, tiers, and cursors.

tools-core/OQL:

- interpret research intent;
- choose structure/discovery/AST/LSP steps;
- apply framework/package entrypoint policy;
- run bounded LSP proof escalation;
- shape packets for agents;
- expose `next.*`, pagination, diagnostics, and missing proof.

## Agent Checklist

Before answering a graph question:

```text
Did I bound the corpus with structure/files?
Did I use ripgrep/file search only for discovery?
Did I inventory declarations/imports/exports/calls with AST?
Did I prove symbol identity/references/calls with LSP when available?
Did I classify same-file/internal/external/test/generated/type-only refs?
Did I check whether each keeper is itself reachable?
Did I preserve missing dynamic/framework/config proof?
Did I return exact next file/line/query?
Did I avoid safe-delete claims from candidate evidence?
```
