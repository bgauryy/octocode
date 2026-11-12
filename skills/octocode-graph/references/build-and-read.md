# Build a graph and read its rows

Load when creating or refreshing a snapshot, or before interpreting rows, weights, confidence or gaps. `$OCTO` is the CLI named in the lobby.

## Create

```bash
$OCTO graphBuild '{"queries":[{"path":"src"}]}'
```

- Use the **same `path`** for build and every query; omit `graph` to read the newest snapshot of exactly that path, or pin the returned `id`.
- An unchanged scope returns its snapshot at once. After edits, `graphQuery` `stale` reports `fresh:false` with a `reingest` call: run it.
- A long build pauses with progress: run `next.expandScan`, or repeat the call to resume. `scopeTooBroad` names narrower scopes: split there, never at a guessed size.
- Check `linkage` and `diagnostics` in the receipt before trusting call answers: linkage counts syntax call sites the engine linked, not semantic recall.

## Read the rows

- **Page first.** Follow `responsePagination.next`, then `next.*`, copying each call unchanged. A count with `hasMore` is a floor.
- **Confidence is categorical binding evidence.** `high` (default) is a strong syntax binding; `medium` adds scoped or ambiguous candidates; `low` adds a bare call's only declaration of its name; member calls on unknown receivers stay in `unresolved`. Ask again at `medium` before reading an empty answer as absence.
- **Weights mean different things; never mix them.** `sites` counts distinct source occurrences behind an edge (connection strength). `pageRank` is structural importance. `commits`/`share` in `cochange` is change coupling from Git history. None is a probability or a runtime frequency.
- **`impact` lists dependents, not breakage.** `direct` means "depends on it directly: inspect first". Only tests or execution prove a break; `testsToRun` and `affectedEntrypoints` say where to look.
- **`issues` are ranked hypotheses.** Read each finding's `evidence` and `controls`, then check it: a cycle with `path`, an unused export with `lspSearch` references, a critical file with `dependents` to its `farthestDepth`.
- **`passes`** is an argument slot (a named function handed to a call), not proof of invocation. Other value uses produce no edge, so a function can be used without any `calls` row.
- **Gaps are listed, not hidden.** `unresolved` gives reasons (external package, unknown receiver, ambiguous). Dynamic dispatch, reflection and string loading are not modeled.
