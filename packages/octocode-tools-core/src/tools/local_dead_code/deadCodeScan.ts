import {
  buildFileGraph,
  DEFAULT_DEAD_CODE_EXCLUDE_DIRS,
} from './graphBuilder.js';
import { resolveEntrypoints } from './entrypoints.js';
import { resolveImportSpecifier } from './importResolver.js';
import {
  computeReachableFiles,
  findStronglyConnectedComponents,
} from './reachability.js';
import type { FileFacts } from './types.js';
import type {
  DeadClusterOutput,
  DeadExportOutput,
  FindDeadCodeQuery as StrictFindDeadCodeQuery,
} from './scheme.js';
import type { WithOptionalMeta } from '../../types/execution.js';

// `path` is passed separately as `rootAbsolutePath`; this module never reads
// `query.path`, so it accepts the same loosely-typed query every other local
// tool threads through its call chain instead of re-deriving the strict,
// path-required Zod-inferred shape.
type FindDeadCodeQuery = WithOptionalMeta<StrictFindDeadCodeQuery>;

export interface DeadCodeScanResult {
  filesScanned: number;
  filesSkipped: number;
  entrypointsResolved: string[];
  deadExports: DeadExportOutput[];
  deadClusters: DeadClusterOutput[];
  warnings: string[];
}

function bindingKey(file: string, name: string): string {
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
        visited
      )
    ) {
      return true;
    }
  }
  return false;
}

/** Is `exportName` retained by anything already proven live? */
function isRetained(
  file: string,
  exportName: string,
  facts: FileFacts,
  entrypointSet: ReadonlySet<string>,
  realImportIndex: ReadonlySet<string>,
  reexportIndex: ReadonlyMap<
    string,
    ReadonlyArray<{ file: string; localName: string }>
  >
): boolean {
  // Same-file usage: some declaration in this file calls it directly.
  if (facts.calls.some(c => c.callee === exportName)) return true;

  // Same-file usage by value reference (spread, property access, argument
  // passing) rather than invocation — `calls` never sees this, so fall back
  // to a whole-word occurrence count against the declaration's own name.
  if ((facts.referenceCounts.get(exportName) ?? 0) > 1) return true;

  // Cross-file usage: someone really imports this name directly, or it's
  // re-exported by a chain that ultimately terminates in a real import (or
  // an entrypoint's public surface).
  return isBindingLive(
    file,
    exportName,
    entrypointSet,
    realImportIndex,
    reexportIndex,
    new Set()
  );
}

export function scanForDeadCode(
  rootAbsolutePath: string,
  query: FindDeadCodeQuery
): DeadCodeScanResult {
  const excludeDir =
    query.excludeDir && query.excludeDir.length > 0
      ? query.excludeDir
      : DEFAULT_DEAD_CODE_EXCLUDE_DIRS;
  const maxFiles = query.maxFiles ?? 20_000;

  const {
    facts,
    fileGraph,
    filesScanned,
    filesSkipped,
    truncated,
    starReexportTargets,
  } = buildFileGraph(rootAbsolutePath, excludeDir, maxFiles);

  const knownFiles = new Set(facts.keys());
  const { entrypoints, warnings } = resolveEntrypoints(
    rootAbsolutePath,
    query.entrypoints,
    query.includeTests ?? true,
    knownFiles
  );

  if (truncated) {
    warnings.push(
      `scan stopped at maxFiles (${maxFiles}) — results are partial, not a full-repo verdict`
    );
  }

  const reachableFiles = computeReachableFiles(fileGraph, entrypoints);
  const entrypointSet = new Set(entrypoints);

  // Precompute each file's `localName -> resolved target file` map once
  // (same resolver the graph builder used), then fold into two reverse
  // indexes so `isRetained`/`isBindingLive` do O(1) lookups per candidate
  // instead of re-scanning every reachable file's imports/re-exports.
  const fileImportsResolved = new Map<string, Map<string, string>>();
  for (const [file, fileFacts] of facts) {
    const localToTarget = new Map<string, string>();
    for (const imp of fileFacts.imports) {
      const target = resolveImportSpecifier(imp.specifier, file, knownFiles);
      if (target) localToTarget.set(imp.localName, target);
    }
    fileImportsResolved.set(file, localToTarget);
  }

  // `${targetFile}::${importedName}` for every real (non-re-export) import
  // reachable code actually makes.
  const realImportIndex = new Set<string>();
  // `${targetFile}::${importedName}` -> re-exporting files and the local
  // name each exposes it under.
  const reexportIndex = new Map<
    string,
    Array<{ file: string; localName: string }>
  >();

  for (const file of reachableFiles) {
    const fileFacts = facts.get(file);
    if (!fileFacts) continue;
    const resolved = fileImportsResolved.get(file);

    for (const imp of fileFacts.imports) {
      const target = resolved?.get(imp.localName);
      if (target) realImportIndex.add(bindingKey(target, imp.importedName));
    }

    for (const rex of fileFacts.namedReexports) {
      const target = resolveImportSpecifier(rex.specifier, file, knownFiles);
      if (!target) continue;
      const key = bindingKey(target, rex.importedName);
      const list = reexportIndex.get(key) ?? [];
      list.push({ file, localName: rex.localName });
      reexportIndex.set(key, list);
    }
  }

  const deadExports: DeadExportOutput[] = [];

  const sccs = findStronglyConnectedComponents(fileGraph);
  const clusterIdByFile = new Map<string, number>();
  const deadClusters: DeadClusterOutput[] = [];
  let clusterId = 0;
  for (const scc of sccs) {
    const allUnreachable = scc.files.every(f => !reachableFiles.has(f));
    if (!allUnreachable) continue;
    const id = clusterId++;
    for (const f of scc.files) clusterIdByFile.set(f, id);
    deadClusters.push({
      id,
      files: scc.files,
      reason:
        'mutually-referencing cluster with no path from any entrypoint — each file looks locally referenced by the others, but the cluster as a whole is unreachable',
    });
  }

  for (const [file, fileFacts] of facts) {
    const isEntrypoint = entrypointSet.has(file);
    const isReachable = reachableFiles.has(file);

    for (const decl of fileFacts.declarations) {
      if (!decl.exported) continue;

      if (!isReachable) {
        const cluster = clusterIdByFile.get(file);
        deadExports.push({
          file,
          name: decl.name,
          kind: decl.kind,
          line: decl.line,
          reason: cluster !== undefined ? 'dead-cluster' : 'unreachable-file',
          ...(cluster !== undefined ? { clusterId: cluster } : {}),
        });
        continue;
      }

      // Entrypoint exports are the public API surface; a file re-published
      // wholesale via `export * from` is treated the same way, since a star
      // re-export republishes every one of its target's exports.
      if (isEntrypoint || starReexportTargets.has(file)) continue;

      const retained = isRetained(
        file,
        decl.name,
        fileFacts,
        entrypointSet,
        realImportIndex,
        reexportIndex
      );
      if (!retained) {
        deadExports.push({
          file,
          name: decl.name,
          kind: decl.kind,
          line: decl.line,
          reason: 'unreferenced-export',
        });
      }
    }
  }

  return {
    filesScanned,
    filesSkipped,
    entrypointsResolved: entrypoints,
    deadExports,
    deadClusters,
    warnings,
  };
}
