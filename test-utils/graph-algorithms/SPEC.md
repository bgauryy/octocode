# Graph algorithm specification

The Rust engine (`tool-graph`, `engine/src/graph`) and the TypeScript graph (`packages/octocode-graph`) each
implement the operations below. There is one specification and one set of cases, `cases.json`; both sides read it
(Rust: `graph_queries_answer_the_shared_algorithm_cases` and `shared_cases_hold_for_the_ring_and_pagerank`; TS:
`tests/algorithms-conformance.test.mjs`). A change to an operation changes this file, `cases.json` and both sides in
one step. Core has no hidden tools, so sharing one implementation needs a new agent-visible tool: deferred.

A case is a directed graph (`nodes`, `edges` as `[source, target]`) and the expected answers. Node ids compare as
strings, code point by code point. The input order of nodes and edges never changes an answer; the TS suite also runs
every case reversed and rotated.

## Operations

| Operation | Definition |
|---|---|
| Cycle groups | Strongly connected components with several nodes, or one node with an edge to itself. A group lists its members in ascending id order. Groups come largest first, then by smallest member id. |
| Ring | The shortest cycle through a node `s` inside its component: breadth-first from `s` along edges to members, until an edge returns to `s`. The ring lists each node once, starts at `s`, and leaves the closing edge implicit (`[s]` for a self-loop). `null` when `s` is on no cycle. With no node given, `s` is the group's smallest member. |
| Fewest-hops path | Breadth-first from `from`; the path lists every node from `from` to `to`. `null` when `to` is unreachable. |
| Reverse reach | Breadth-first along edges backwards from the seeds; each reached node has its hop count. Seeds are never listed, even when a cycle reaches them again. |
| PageRank | Damping 0.85; rank flows along an edge to its target, split by edge weight (unit weights here); a node with no out-weight spreads its rank evenly over all ranked nodes; ranks sum to 1. The expected values are the exact fixed point; an implementation agrees within 1e-4 (the Rust engine stops at an L1 change below 1e-6, the TS graph at 1e-10). |

**Tie rule.** Breadth-first search visits a node's neighbours in ascending id order and keeps the first
discoverer of each node. So of several shortest rings or paths, the one whose first differing node has the smallest
id wins (`tie-two-shortest-rings`, `shuffled-input-order`).

PageRank ranks the nodes it is given. The engine's `hubs` ranking gives it the nodes that an admitted edge touches;
the TS graph gives it every file. The cases hold no isolated node, where the two agree. Both match the literature
(Page and Brin: d = 0.85, power iteration, dangling mass spread uniformly), so neither was changed.

## What differs on purpose, and is not in the cases

| Left out | Why |
|---|---|
| Weighted strongest path (TS) | Weights are TS import and call evidence; the engine's path is fewest-hops over typed edges. |
| Risk annotations, `runtime` load verdicts | Each side annotates cycles from its own evidence. |
| Module-tree exclusion (engine) | The engine drops `rust-module` and data edges before finding cycles; the TS graph has no such edge kinds. |
| Edge counts | The engine stores one edge per (source, kind, target) plus package edges; the TS graph keeps one per file pair and lists packages apart (`externals`). Reachability is the same. |
