// Retention / binding-liveness analysis for the dead-code scan: is one
// file's exported binding consumed by anything already proven live?
// Extracted from deadCodeScan.ts — pure functions over the graph indexes the
// scan precomputes (realImportIndex, reexportIndex, starReexporters).
import type { FileFacts } from '../../graph/types.js';

export function bindingKey(file: string, name: string): string {
  return `${file}::${name}`;
}

/**
 * Is `(file, name)` — a declaration or a re-export's own local binding —
 * consumed by something that isn't itself just a further pass-through?
 * Live if: it's the public surface of an entrypoint, something really
 * imports it directly, or it's re-exported by a file whose own binding of
 * the re-export is (recursively) live. A cycle guard handles re-export
 * loops; `visited` is per top-level call, not shared across candidates.
 */
function isBindingLive(
  file: string,
  name: string,
  entrypointSet: ReadonlySet<string>,
  realImportIndex: ReadonlySet<string>,
  reexportIndex: ReadonlyMap<
    string,
    ReadonlyArray<{ file: string; localName: string }>
  >,
  starReexporters: ReadonlyMap<string, ReadonlyArray<string>>,
  visited: Set<string>
): boolean {
  const key = bindingKey(file, name);
  if (visited.has(key)) return false;
  if (entrypointSet.has(file)) return true;
  if (realImportIndex.has(key)) return true;
  visited.add(key);
  for (const reexporter of reexportIndex.get(key) ?? []) {
    if (
      isBindingLive(
        reexporter.file,
        reexporter.localName,
        entrypointSet,
        realImportIndex,
        reexportIndex,
        starReexporters,
        visited
      )
    ) {
      return true;
    }
  }
  // Star hop: `export * from file` republishes EVERY export of `file` under
  // the same name — liveness continues at the star-re-exporting barrel. This
  // edge carries no per-name entry in `reexportIndex` (there is no name to
  // index), so without this hop a named re-export chained through a
  // star-re-exported barrel (types.ts → barrel → `export *` → entrypoint)
  // false-positives as an unreferenced export.
  for (const starReexporter of starReexporters.get(file) ?? []) {
    if (
      isBindingLive(
        starReexporter,
        name,
        entrypointSet,
        realImportIndex,
        reexportIndex,
        starReexporters,
        visited
      )
    ) {
      return true;
    }
  }
  return false;
}

/**
 * Symbol-level liveness for one file's exported declarations.
 *
 * Seed: exports proven live by cross-file evidence (real import, live
 * re-export chain, star hop — `isBindingLive`). Then propagate same-file
 * call edges to a fixpoint: a call only retains its callee when the CALLER is
 * itself live — two dead exports calling each other must not retain each
 * other. Non-exported callers are conservatively treated as live (their
 * liveness is not tracked, and module-level calls carry no caller edge at
 * all). The lexical whole-word fallback (value references — spreads,
 * property access, arguments — that `calls` never sees) still applies, but
 * call-site occurrences attributable to DEAD exported callers are subtracted
 * first so a dead caller's call site no longer lexically retains its callee.
 * Both propagation rules are monotone (live only grows, the subtraction only
 * shrinks), so the fixpoint terminates.
 */
export function computeLiveExportedNames(
  file: string,
  facts: FileFacts,
  entrypointSet: ReadonlySet<string>,
  realImportIndex: ReadonlySet<string>,
  reexportIndex: ReadonlyMap<
    string,
    ReadonlyArray<{ file: string; localName: string }>
  >,
  starReexporters: ReadonlyMap<string, ReadonlyArray<string>>
): Set<string> {
  const exportedNames = new Set(
    facts.declarations.filter(d => d.exported).map(d => d.name)
  );
  const live = new Set<string>();
  for (const name of exportedNames) {
    if (
      isBindingLive(
        file,
        name,
        entrypointSet,
        realImportIndex,
        reexportIndex,
        starReexporters,
        new Set()
      )
    ) {
      live.add(name);
    }
  }

  const callerIsLive = (caller: string): boolean =>
    !exportedNames.has(caller) || live.has(caller);

  let changed = true;
  while (changed) {
    changed = false;
    for (const name of exportedNames) {
      if (live.has(name)) continue;
      const callsToName = facts.calls.filter(c => c.callee === name);

      // Same-file usage: a LIVE declaration (or an untracked non-exported
      // one) calls it directly. A self-call never retains.
      if (callsToName.some(c => c.caller !== name && callerIsLive(c.caller))) {
        live.add(name);
        changed = true;
        continue;
      }

      // Lexical fallback for value references, minus occurrences already
      // explained by call sites inside dead exported callers.
      const deadCallerCallCount = callsToName.filter(
        c => c.caller === name || !callerIsLive(c.caller)
      ).length;
      const adjustedOccurrences =
        (facts.referenceCounts.get(name) ?? 0) - deadCallerCallCount;
      if (adjustedOccurrences > 1) {
        live.add(name);
        changed = true;
      }
    }
  }
  return live;
}
