# Build a graph and read its rows

Load when creating or refreshing a snapshot, or before interpreting rows, weights, confidence or gaps.

## Query first

Ask for the relationship on the exact scope, for example:

```bash
octocode-graph graphQuery '{"queries":[{"path":"packages/api","operation":"deps","target":"src/index.ts"}]}'
```

The first page checks freshness. A current snapshot omits the freshness warning. A missing snapshot (`notFound` with a build suggestion) or `freshness.fresh: false` carries a related build. Use its scope and options, follow build continuations, then repeat the original query pinned to the new ID. For an exact call target, verify the selected symbol with `lspSearch` and read the deciding lines with `localFetch`.

## Create

Use `octocode-graph` from the target workspace. Check the operation’s inputs with `octocode-graph schema graphQuery`. To build over a `ghCloneRepo` checkout outside the workspace, configure `OCTOCODE_GRAPH_HOME` in the home environment first.

```bash
octocode-graph graphBuild '{"queries":[{"path":"."}]}'
```

- Use the **same `path`** for build and every query; omit `graph` to read the newest snapshot of exactly that path, or pass the build row’s `data.graph` as `graph` to pin the snapshot.
- An unchanged scope returns its snapshot at once. After edits, `graphQuery` `stale` reports `fresh:false` with a `reingest` call: run it.
- A long build pauses with progress: run `next.expandScan`, or repeat the call to resume. `scopeTooBroad` names narrower scopes: split there, never at a guessed size.
- Check `linkage` and `diagnostics` in the receipt before trusting call answers: linkage counts syntax call sites the engine linked, not semantic recall.

## Read the rows

- **Page first.** A reply with a top-level `next` is unfinished: run it until none is left, then the evidence branches under a row's `next.*`, copying each call unchanged. Join `rowPart` fragments for the same result in order: append new list elements, merge the array element named by `continues`, and append text at the JSON pointer in `text`. Keep repeated identities once. A stated `pagination.total` is exact; check `isPartial`, `partialReasons` and skipped scope before treating an answer as complete.
- **Confidence is categorical binding evidence.** `high` (default) is a strong syntax binding; `medium` adds scoped or ambiguous candidates; `low` adds a bare call's only declaration of its name; member calls on unknown receivers stay in `unresolved`. Ask again at `medium` before reading an empty answer as absence.
- **Weights mean different things; never mix them.** `sites` counts distinct source occurrences behind an edge (connection strength). `pageRank` is structural importance. `commits`/`share` in `cochange` is change coupling from Git history. None is a probability or a runtime frequency.
- **`impact` lists dependents, not breakage.** `direct` means "depends on it directly: inspect first". Only tests or execution prove a break; the summary counts `testsToRun` and `affectedEntrypoints`, and `next.testsToRun` and `next.affectedEntrypoints` list where to look.
- **`issues` are ranked hypotheses.** Read each finding's `evidence` and `controls`, then check it: a cycle with `path`, an unused export with `lspSearch` references, a critical file with `dependents` to its `farthestDepth`.
- **`passes`** is an argument slot (a named function handed to a call), not proof of invocation. Other value uses produce no edge, so a function can be used without any `calls` row.
- **Gaps are listed, not hidden.** `unresolved` gives reasons (external package, unknown receiver, ambiguous). Dynamic dispatch, reflection and string loading are not modeled.
- **Direction and depth select the question.** `deps`/`callees` follow outgoing edges; `dependents`/`callers` follow incoming edges. Their default is one hop; `impact` defaults to three. Increase `depth` to cover the claim; paging exhausts the selected depth, not the whole transitive graph. Pin `graph` when comparing queries, and use `keep` at build time to retain baselines across later builds.
