// When an import takes effect. The engine states it (graphQuery `load` on each import witness and import diagnostic,
// from its kind table in packages/octocode-native/crates/tool-graph/src/ast_graph/edges.rs), and a section keeps it
// per kind (`loads`), which wins here. The table below mirrors the engine's for graphs built before it stated loads:
//   eager     while the importing module loads (static import, value re-export, require)
//   deferred  later, on a call (dynamic import(), a Python import inside a function): a lazy boundary
//   type      never: erased before run time (import type, export type, an import TypeScript elides)
//   compile   at compile or link time (Rust use and paths, Go, Java, C includes)
//   data      embedded as bytes (include_bytes!)
//   unknown   not established: a parser fallback read it, a TypeScript value import used only as a type whose emit
//             the project's settings decide, or an import stitched between chunks by an engine that did not say its kind
// One edge between two files is as strong as its strongest import: `import { type T, v }` loads. A file pair joined
// only by calls is `compile` in a language that binds at compile time (query-index.ts moduleLoading).

export type Load =
  'eager' | 'deferred' | 'type' | 'compile' | 'data' | 'unknown';

const KINDS: Record<string, Load> = {
  'static-import': 'eager',
  'named-reexport': 'eager',
  'star-reexport': 'eager',
  'commonjs-require': 'eager',
  'create-require': 'eager',
  'python-import': 'eager',
  'dynamic-import': 'deferred',
  'python-lazy-import': 'deferred',
  'type-import': 'type',
  'type-named-reexport': 'type',
  'type-star-reexport': 'type',
  'elided-import': 'type',
  'recovered-import': 'unknown',
  'recovered-named-reexport': 'unknown',
  'recovered-star-reexport': 'unknown',
  'type-use-import': 'unknown',
  'rust-use': 'compile',
  'rust-path': 'compile',
  'rust-include': 'compile',
  'go-import': 'compile',
  'java-import': 'compile',
  'java-same-package': 'compile',
  'c-include': 'compile',
  'namespace-use': 'compile',
  'crate-path': 'compile',
  'rust-include-data': 'data',
};

/** How stitching joined an import between chunks; not a kind, so it settles nothing alone. */
const STITCH_VIA = new Set(['relative', 'package']);

// Strongest first, as the engine ranks them (edges.rs `strength`).
const STRENGTH: Record<Load, number> = {
  eager: 0,
  compile: 0,
  deferred: 1,
  unknown: 2,
  type: 3,
  data: 4,
};

/**
 * The load of one edge kind; a kind outside the table is not established. How stitching joined chunks and how a
 * package is declared (`external`, `external-dev`, …: the `detail` of a package import) are not kinds: they settle
 * nothing alone.
 */
export const loadOfKind = (
  kind: string,
  recorded?: Readonly<Record<string, Load>>
): Load | null =>
  STITCH_VIA.has(kind) || /^external(-|$)/.test(kind)
    ? null
    : (recorded?.[kind] ?? KINDS[kind] ?? 'unknown');

/** How strongly a load ties two files, strongest (0) first: eager and compile-time imports tie. */
export const strengthOf = (load: Load) => STRENGTH[load];

/**
 * The load of an import edge from every kind it records: the strongest one; none recorded is `unknown`. `recorded`
 * holds the load the engine stated for each kind (an edge's or package's `loads`), which wins over the mirror.
 */
export function loadOf(
  kinds: readonly string[] | undefined,
  recorded?: Readonly<Record<string, Load>>
): Load {
  let best: Load | null = null;
  for (const kind of kinds ?? []) {
    const load = loadOfKind(kind, recorded);
    if (load && (best === null || STRENGTH[load] < STRENGTH[best])) best = load;
  }
  return best ?? 'unknown';
}

/** Which loads a walk keeps: `runtime` drops what never runs (type-only); `eager` also drops lazy boundaries. */
export type LoadMode = 'runtime' | 'eager';

export function keeps(mode: LoadMode, load: Load) {
  if (load === 'type') return false;
  return mode === 'runtime' || load !== 'deferred';
}
