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
  starReexporters: ReadonlyMap<string, ReadonlyArray<string>>
): boolean {
  const visited = new Set<string>();
  const pending = [{ file, name }];
  while (pending.length > 0) {
    const current = pending.pop() as { file: string; name: string };
    const key = bindingKey(current.file, current.name);
    if (visited.has(key)) continue;
    visited.add(key);
    if (entrypointSet.has(current.file) || realImportIndex.has(key))
      return true;
    for (const reexporter of reexportIndex.get(key) ?? []) {
      pending.push({ file: reexporter.file, name: reexporter.localName });
    }
    for (const starReexporter of starReexporters.get(current.file) ?? []) {
      pending.push({ file: starReexporter, name: current.name });
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
        starReexporters
      )
    ) {
      live.add(name);
    }
  }

  const calleesByCaller = new Map<string, string[]>();
  const exportedCallerCounts = new Map<string, number>();
  for (const call of facts.calls) {
    const callees = calleesByCaller.get(call.caller) ?? [];
    callees.push(call.callee);
    calleesByCaller.set(call.caller, callees);
    if (exportedNames.has(call.caller)) {
      exportedCallerCounts.set(
        call.callee,
        (exportedCallerCounts.get(call.callee) ?? 0) + 1
      );
    }
  }
  const pending = [...live];
  const markLive = (name: string): void => {
    if (!exportedNames.has(name) || live.has(name)) return;
    live.add(name);
    pending.push(name);
  };
  for (const [caller, callees] of calleesByCaller) {
    if (exportedNames.has(caller)) continue;
    for (const callee of callees) markLive(callee);
  }
  for (const name of exportedNames) {
    const adjustedOccurrences =
      (facts.referenceCounts.get(name) ?? 0) -
      (exportedCallerCounts.get(name) ?? 0);
    if (adjustedOccurrences > 1) markLive(name);
  }
  for (let cursor = 0; cursor < pending.length; cursor++) {
    const caller = pending[cursor] as string;
    for (const callee of calleesByCaller.get(caller) ?? []) {
      if (callee !== caller) markLive(callee);
    }
  }
  return live;
}
