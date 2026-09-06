import {
  buildFileGraph,
  resolveGraphExcludeDirs,
  type WalkResult,
} from '../../graph/buildFileGraph.js';
import { isTestFilePath, resolveEntrypoints } from './entrypoints.js';
import {
  computeReachableFiles,
  findStronglyConnectedComponents,
} from '../../graph/reachability.js';
import { bindingKey, computeLiveExportedNames } from './retention.js';
import type {
  DeadClusterOutput,
  DeadExportOutput,
  AnalyzeGraphQuery,
} from './scheme.js';

type DeadCodeScanOptions = Partial<
  Pick<
    Extract<AnalyzeGraphQuery, { operation: 'deadCode' }>,
    'excludeDir' | 'maxFiles' | 'rustWorkspace' | 'entrypoints' | 'includeTests'
  >
>;

export interface DeadCodeScanResult {
  filesScanned: number;
  filesSkipped: number;
  entrypointsResolved: string[];
  deadExports: DeadExportOutput[];
  deadClusters: DeadClusterOutput[];
  warnings: string[];
  /**
   * Present (and always `"low"`) when the scan's reachability base is shaky, so
   * the raw candidate count should not be trusted as-is. Triggers, each
   * with its own explanatory `warnings` entry:
   *  - `entrypointsResolved` came entirely from the test-file heuristic (no
   *    package.json main/exports/bin matched and no explicit `entrypoints`),
   *    so every export in an otherwise-live file reads as "dead"; re-scope to
   *    the real entrypoint-bearing package or pass `entrypoints` explicitly.
   *  - one or more resolved entrypoints parsed to zero edges (a native
   *    extractor parse failure on that file), so reachability can't leave them
   *    and files they should reach show as false "unreachable-file".
   *  - The scan was capped, files were skipped, or coverage diagnostics report
   *    parse recovery or unsupported/unresolved import linking. Missing edges
   *    can hide live paths even when entrypoints contain valid declarations.
   */
  confidence?: 'low';
}

export async function scanForDeadCode(
  rootAbsolutePath: string,
  query: DeadCodeScanOptions,
  builtGraph?: WalkResult
): Promise<DeadCodeScanResult> {
  const excludeDir = resolveGraphExcludeDirs(query.excludeDir);
  const maxFiles = query.maxFiles ?? 20_000;

  const {
    facts,
    fileGraph,
    filesScanned,
    filesSkipped,
    truncated,
    coverage,
    namespaceImportTargets,
    starReexporters,
  } =
    builtGraph ??
    (await buildFileGraph(
      rootAbsolutePath,
      excludeDir,
      maxFiles,
      query.rustWorkspace
    ));

  const knownFiles = new Set(facts.keys());
  const {
    entrypoints,
    warnings,
    lowConfidence: lowConfidenceEntrypoints,
  } = resolveEntrypoints(
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
  if (filesSkipped > 0) {
    warnings.push(
      `${filesSkipped} file(s) could not be read or parsed within the native graph bounds — results are partial, not a full-repo verdict`
    );
  }
  const incompleteCoverage =
    coverage?.diagnostics.some(item => item.code !== 'syntax-only') ?? false;
  if (incompleteCoverage) {
    warnings.push(
      'Incomplete import linking or parse recovery can hide live paths. Dead-code results are low-confidence candidates and require semantic reference verification.'
    );
  }

  // A resolved entrypoint that extracted to NO outgoing edges and NO
  // declarations of its own almost always means the native extractor failed to
  // parse it (a known failure mode on some Flow-typed `.js` files), not a
  // genuinely self-contained leaf. Reachability can't leave such an entrypoint,
  // so every file it should have reached reads as a false "unreachable-file".
  // Detect it and lower confidence rather than emitting a confidently-wrong
  // dead-code verdict.
  const zeroSignalEntrypoints = entrypoints.filter(ep => {
    const epFacts = facts.get(ep);
    const node = fileGraph.get(ep);
    return (
      (node?.importsFiles.size ?? 0) === 0 &&
      (epFacts?.declarations.length ?? 0) === 0 &&
      (epFacts?.namedReexports.length ?? 0) === 0
    );
  });
  const extractorFailedEntrypoints = zeroSignalEntrypoints.length > 0;
  if (extractorFailedEntrypoints) {
    warnings.push(
      `${zeroSignalEntrypoints.length} entrypoint file(s) parsed to no imports/exports (likely an extractor parse failure on that file) — reachability from them is unreliable, so "unreachable-file" results here are low-confidence and may be false positives: ${zeroSignalEntrypoints.join(', ')}`
    );
  }

  const reachableFiles = computeReachableFiles(fileGraph, entrypoints);
  const entrypointSet = new Set(entrypoints);
  const publicSurfaceSet = new Set([
    ...entrypointSet,
    ...namespaceImportTargets,
  ]);
  const shouldReportFile = (file: string): boolean =>
    query.includeTests !== false || !isTestFilePath(file);

  // A file reachable only through a string-literal dynamic import() has
  // lower-confidence reachability than one reachable through a static
  // import: the dynamic path is proven by parsing the specifier, not by
  // proving how the resulting namespace object's properties get used.
  // Surface this instead of silently treating it as equally certain —
  // recompute reachability over the static-edges-only graph and diff.
  const staticOnlyGraph = new Map(
    [...fileGraph].map(([file, node]) => [
      file,
      {
        relativePath: node.relativePath,
        importsFiles: new Set(
          [...node.importsFiles].filter(
            target => !node.dynamicImportsFiles.has(target)
          )
        ),
        dynamicImportsFiles: new Set<string>(),
        edgeKinds: new Map(
          [...node.edgeKinds]
            .map(
              ([target, kinds]) =>
                [
                  target,
                  new Set([...kinds].filter(kind => kind !== 'dynamic-import')),
                ] as const
            )
            .filter(([, kinds]) => kinds.size > 0)
        ),
      },
    ])
  );
  const staticReachableFiles = computeReachableFiles(
    staticOnlyGraph,
    entrypoints
  );
  const dynamicImportOnlyReachable = [...reachableFiles]
    .filter(file => !staticReachableFiles.has(file))
    .sort();
  if (dynamicImportOnlyReachable.length > 0) {
    warnings.push(
      `${dynamicImportOnlyReachable.length} file(s) reachable only through a dynamic import() — lower confidence than static analysis, verify with lspGetSemantics before treating as proof: ${dynamicImportOnlyReachable.join(', ')}`
    );
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

    for (const imp of fileFacts.imports) {
      const target = imp.resolvedTarget;
      if (target) realImportIndex.add(bindingKey(target, imp.importedName));
    }

    for (const rex of fileFacts.namedReexports) {
      const target = rex.resolvedTarget;
      if (!target) continue;
      const key = bindingKey(target, rex.importedName);
      const list = reexportIndex.get(key) ?? [];
      list.push({ file, localName: rex.localName });
      reexportIndex.set(key, list);
    }
  }

  // Star edges only count when the star-re-exporting file is itself
  // reachable: an unreachable barrel's `export *` republishes nothing that
  // any live code can see. Filter both the reverse map (used by the
  // `isBindingLive` star hop) and the "whole surface is public" target set
  // to reachable re-exporters.
  const liveStarReexporters = new Map<string, string[]>();
  for (const [target, reexporters] of starReexporters) {
    const alive = reexporters.filter(f => reachableFiles.has(f));
    if (alive.length > 0) liveStarReexporters.set(target, alive);
  }
  const liveStarReexportTargets = new Set(liveStarReexporters.keys());

  const deadExports: DeadExportOutput[] = [];

  const sccs = findStronglyConnectedComponents(fileGraph);
  const clusterIdByFile = new Map<string, number>();
  const deadClusters: DeadClusterOutput[] = [];
  let clusterId = 0;
  for (const scc of sccs) {
    const reportableFiles = scc.files.filter(shouldReportFile);
    if (reportableFiles.length === 0) continue;
    const allUnreachable = reportableFiles.every(f => !reachableFiles.has(f));
    if (!allUnreachable) continue;
    const id = clusterId++;
    for (const f of reportableFiles) clusterIdByFile.set(f, id);
    deadClusters.push({
      id,
      files: reportableFiles,
      reason:
        'mutually-referencing cluster with no path from any entrypoint — each file looks locally referenced by the others, but the cluster as a whole is unreachable',
    });
  }

  const liveNamesByFile = new Map<string, Set<string>>();

  for (const [file, fileFacts] of facts) {
    if (!shouldReportFile(file)) continue;
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
      if (
        isEntrypoint ||
        liveStarReexportTargets.has(file) ||
        namespaceImportTargets.has(file)
      )
        continue;

      let liveNames = liveNamesByFile.get(file);
      if (!liveNames) {
        liveNames = computeLiveExportedNames(
          file,
          fileFacts,
          publicSurfaceSet,
          realImportIndex,
          reexportIndex,
          liveStarReexporters
        );
        liveNamesByFile.set(file, liveNames);
      }
      if (!liveNames.has(decl.name)) {
        deadExports.push({
          file,
          name: decl.name,
          kind: decl.kind,
          line: decl.line,
          reason: 'unreferenced-export',
          // Which negative-evidence path concluded "dead": a re-export chain
          // that terminated without a consumer is more fragile than plain
          // "nobody imports it" — LSP-verify those candidates first.
          viaHeuristic: reexportIndex.has(bindingKey(file, decl.name))
            ? 'reexport-chain'
            : 'lexical-count',
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
    ...(lowConfidenceEntrypoints ||
    extractorFailedEntrypoints ||
    incompleteCoverage ||
    truncated ||
    filesSkipped > 0
      ? { confidence: 'low' as const }
      : {}),
  };
}
