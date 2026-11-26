Load when creating or refreshing a native snapshot, or before interpreting its rows, weights, confidence or gaps.

# Build a graph and read its rows

## Query first

Ask for the relationship on the exact scope, for example:

```bash
octocode-graph graphQuery '{"queries":[{"path":"packages/api","operation":"deps","target":"src/index.ts"}]}' --all-pages
```

The first page checks freshness. A current snapshot omits the freshness warning. A missing snapshot (`notFound` with a build suggestion) or `freshness.fresh: false` carries a related build. Use its scope and options, follow build continuations, then repeat the original query pinned to the new ID. For an ambiguous call target, check the binding with `lspSearch` or read the deciding source span.

## Create

Run from the target workspace. For unfamiliar inputs, `octocode-graph schema graphBuild` or `octocode-graph schema graphQuery` describes the installed version.

```bash
octocode-graph graphBuild '{"queries":[{"path":"."}]}'
```

- Use the **same `path`** for build and every query; omit `graph` to read the newest snapshot of exactly that path, or pass the build row’s `data.graph` as `graph` to pin the snapshot.
- An unchanged scope returns its snapshot at once. After edits, `graphQuery` `stale` reports `fresh:false` with a `reingest` call: run it.
- A long build pauses with progress: run `next.expandScan`, or repeat the call to resume. `scopeTooBroad` names narrower scopes: split there, never at a guessed size.
- Check `linkage` and `diagnostics` in the receipt before trusting call answers: linkage counts syntax call sites the engine linked, not semantic recall.

## Read the rows

- **Read every page with `--all-pages`.** `octocode-graph graphQuery '<json>' --all-pages` follows the reply's `next` and every evidence branch under a row's `next.*` (cycle members, `testsToRun`, `affectedEntrypoints`, summary and baseline lists), joins `rowPart` fragments, checks each list against its stated count and prints one JSON reply with no `next`: each row once, with every page's warnings. A joined evidence list takes the place of its count (`summary.testsToRun` becomes the list). It starts from the `{"queries":[…]}` call, not a cursor, and exits as the tool does (an error row stays in the reply). A stated `pagination.total` is exact; check `isPartial`, `partialReasons` and skipped scope before treating an answer as complete.
- **Confidence is categorical binding evidence.** `high` (default) is a strong syntax binding; `medium` adds scoped or ambiguous candidates; `low` adds a bare call's only declaration of its name; member calls on unknown receivers stay in `unresolved`. Ask again at `medium` before reading an empty answer as absence.
- **Weights mean different things; never mix them.** `sites` counts distinct source occurrences behind an edge (connection strength). `pageRank` is structural importance. `commits`/`share` in `cochange` is change coupling from Git history. None is a probability or a runtime frequency.
- **`impact` lists dependents, not breakage.** `direct` means "depends on it directly: inspect first". Only tests or execution prove a break; the summary counts `testsToRun` and `affectedEntrypoints`, and `next.testsToRun` and `next.affectedEntrypoints` list where to look.
- **`issues` are ranked hypotheses.** Read each finding's `evidence` and `controls`, then check it: a cycle with `path`, an unused export or other dead-code finding with `lspSearch` references, a critical file with `dependents` to its `farthestDepth`.
- **`passes`** is an argument slot (a named function handed to a call), not proof of invocation. Other value uses produce no edge, so a function can be used without any `calls` row.
- **Gaps are listed, not hidden.** `unresolved` gives reasons (external package, unknown receiver, ambiguous). Dynamic dispatch, reflection and string loading are not modeled.
- **Direction and depth select the question.** `deps`/`callees` follow outgoing edges; `dependents`/`callers` follow incoming edges. Their default is one hop; `impact` defaults to three. Increase `depth` to cover the claim; paging exhausts the selected depth, not the whole transitive graph. Pin `graph` when comparing queries, and use `keep` at build time to retain baselines across later builds.
