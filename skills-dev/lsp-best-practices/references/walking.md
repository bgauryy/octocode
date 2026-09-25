# Walking: definition chains, call and type graphs

Load when implementing or changing a multi-hop traversal: a definition chain, callers/callees depth, a type hierarchy, or reference expansion. Why: a walk that is unbounded explodes on hot symbols. A walk without structure turns a graph into a flat pile that can't be read.

## Definition chain
Follow `definition` hops with a visited set and a hop cap, and stop when a hop doesn't advance (a cycle, or the same target again). Each hop target must be opened (it costs a document slot) and authorized by the read policy. Keep retries narrow: octocode retries only the TS cold same-file-import-alias case, once, after 50 ms.

## Call hierarchy: the target shape
```
roots    = prepareCallHierarchy(anchor)                     # level 0
seen     = {key(r) for r in roots}                          # key = (canonical uri, selectionRange)
frontier = roots
for level in 1..=depth:                                     # BFS: one level at a time
    results = join_bounded(frontier, incoming|outgoing, concurrency=K, per_level_timeout)
    for (parent, call) in results:
        node = call.from (incoming) | call.to (outgoing)
        emit Edge{parent: key(parent), node, level, sites: fromRanges}   # one edge per (parent,node)
        if key(node) in seen: continue                      # cycle or diamond: edge kept, not expanded
        if nodes >= MAX_NODES: truncated = true; continue
        if !in_scope(node): mark external; continue         # std / node_modules / GOROOT
        seen += key(node); next += node
    frontier = next
return {edges, truncated, failures, continuation}
```
- **Key nodes by `(canonical uri, selectionRange)`.** Not by name, which collides across overloads and modules, and not by the whole serialized item, whose `detail` and `data` vary.
- **Every edge carries `level` and `parent`.** Without them a depth-N answer can't be read as a graph. Merge the `fromRanges` of repeated (parent, node) pairs into one edge.
- **Cap depth, total nodes, and per-node fan-out.** Report `truncated` with an executable continuation (start again from the frontier), never a silent cut. Enforce caps in code, not only in the schema.
- **Level concurrency:** issue each level's requests concurrently under a small semaphore. Responses match by id, so order doesn't matter.
- **Failures are data.** Collect per-node errors. A partial walk is a result with `expansionFailed` and a retry continuation. A walk where every node failed is an error.
- **Agents walk hop by hop.** Serena, Claude Code's LSP tool, and mcp-language-server expose one level per call. A server-side multi-level walk earns its place only if it returns a real tree.

## Type hierarchy
The same BFS over `supertypes`/`subtypes`, with the same key and caps. Diamonds (interfaces) are normal, so keep the edge and don't expand the node twice.

## Reference expansion
Cap the total number of locations and page them. Sort deterministically and dedupe by identity. Group by file for fan-out questions. Alias or re-export recovery (import scanning) runs under explicit file caps and is labeled as recovered, not native.

## Checklist
| Check | Why |
|---|---|
| BFS, not DFS | level-ordered output; `depth` means what it says |
| Identity key, not JSON | real dedupe and cycle detection |
| Edges carry level and parent | readable graph |
| Node, fan-out, and depth caps in code | bounded cost on hot symbols |
| `truncated` + continuation | no silent cuts |
| Output positions converted once | callers and references agree on line numbers |

Next: octocode's implementation of this shape is `R/walk.rs` (see `references/octocode-engine-map.md`); its history is in `references/octocode-known-defects.md`.
