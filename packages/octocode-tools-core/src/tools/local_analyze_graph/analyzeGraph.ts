import {
  buildFileGraph,
  resolveGraphExcludeDirs,
} from '../../graph/buildFileGraph.js';
import { scanForDeadCode } from './deadCodeScan.js';
import { resolveEntrypoints } from './entrypoints.js';
import {
  findShortestPath,
  normalizeGraphFile,
  reverseGraph,
  traverseGraph,
  collectGraphEdgeKinds,
} from '../../graph/operations.js';
import {
  computeReachableFiles,
  findStronglyConnectedComponents,
} from '../../graph/reachability.js';
import {
  computeImmediateDominators,
  condenseGraph,
  findTransitiveEdges,
  withoutTypeOnlyEdges,
} from '../../graph/advancedOperations.js';
import {
  componentLayerMap,
  describeCycleWitness,
} from '../../graph/cycleOperations.js';
import type {
  AnalyzeGraphContext,
  AnalyzeGraphOutput,
  AnalyzeGraphQuery,
} from './analysisTypes.js';
import { finalizeGraphOutput, paginateGraphResults } from './pagination.js';

const DEFAULT_MAX_FILES = 20_000;
const DEFAULT_DEPTH = 1;
export function summarizeEntrypoints(
  entrypoints: string[]
): Record<string, unknown> {
  return {
    // This is the only structured list of resolved entrypoints in the graph
    // response. Slicing it made the omitted values unreachable despite a count
    // and truncation flag, so preserve the complete list.
    entrypointsResolved: entrypoints,
    entrypointsResolvedCount: entrypoints.length,
  };
}
function errorOutput(
  query: AnalyzeGraphQuery,
  message: string
): AnalyzeGraphOutput {
  return {
    status: 'error',
    error: message,
    errorCode: 'invalidGraphQuery',
    operation: query.operation,
    path: query.path,
    results: [],
  };
}
export async function analyzeGraph(
  query: AnalyzeGraphQuery,
  context: AnalyzeGraphContext = {}
): Promise<AnalyzeGraphOutput> {
  const excludeDir = resolveGraphExcludeDirs(query.excludeDir);
  const maxFiles = query.maxFiles ?? DEFAULT_MAX_FILES;
  const built = await (context.getGraph?.(query.path, excludeDir, maxFiles) ??
    buildFileGraph(query.path, excludeDir, maxFiles));
  const finalize = (
    output: AnalyzeGraphOutput,
    why: string
  ): AnalyzeGraphOutput =>
    finalizeGraphOutput(output, query, built.truncated, why);

  if (query.operation === 'deadCode') {
    const scan = await scanForDeadCode(query.path, query, built);
    const page = paginateGraphResults(
      scan.deadExports as unknown as Array<Record<string, unknown>>,
      query
    );
    const pageClusterIds = new Set(
      page.results
        .map(result => result.clusterId)
        .filter((id): id is number => typeof id === 'number')
    );
    return finalize(
      {
        operation: query.operation,
        path: query.path,
        filesScanned: scan.filesScanned,
        filesSkipped: scan.filesSkipped,
        ...page,
        summary: {
          ...summarizeEntrypoints(scan.entrypointsResolved),
          deadClusters: scan.deadClusters
            .filter(cluster => pageClusterIds.has(cluster.id))
            .map(cluster => ({
              ...cluster,
              files: cluster.files,
              size: cluster.files.length,
              edgeKinds: collectGraphEdgeKinds(built.fileGraph, cluster.files),
              confidence: 'syntactic',
            })),
          deadClusterCount: scan.deadClusters.length,
          deadExportCount: scan.deadExports.length,
        },
        ...(scan.warnings.length > 0 ? { warnings: scan.warnings } : {}),
        ...(scan.confidence ? { confidence: scan.confidence } : {}),
      },
      'Continue dead-code candidates.'
    );
  }

  const warnings = built.truncated
    ? [`scan stopped at maxFiles (${maxFiles}) — graph results are partial`]
    : [];
  const base = {
    operation: query.operation,
    path: query.path,
    filesScanned: built.filesScanned,
    filesSkipped: built.filesSkipped,
  };

  if (query.operation === 'cycles') {
    const condensed = condenseGraph(built.fileGraph);
    const layerByComponent = componentLayerMap(condensed.layers);
    const redundantEdges = findTransitiveEdges(condensed.edges);
    const runtimeGraph = withoutTypeOnlyEdges(built.fileGraph);
    const runtimeComponents = findStronglyConnectedComponents(runtimeGraph).map(
      component => [...component.files].sort()
    );
    const cycleComponents = condensed.components.filter(
      component =>
        component.length > 1 ||
        (built.fileGraph
          .get(component[0] as string)
          ?.importsFiles.has(component[0] as string) ??
          false)
    );
    const items = cycleComponents
      .map(allFiles => {
        const componentId = condensed.componentOf.get(allFiles[0] as string);
        const memberSet = new Set(allFiles);
        const containedRuntimeCycles = runtimeComponents.filter(component =>
          component.every(file => memberSet.has(file))
        );
        const cycleEdges = describeCycleWitness(built.fileGraph, memberSet);
        const runtimeCycleEdges = describeCycleWitness(runtimeGraph, memberSet);
        const outgoing =
          componentId === undefined
            ? []
            : [...(condensed.edges.get(componentId) ?? [])].sort(
                (a, b) => a - b
              );
        return {
          files: allFiles,
          size: allFiles.length,
          edgeKinds: collectGraphEdgeKinds(built.fileGraph, allFiles),
          runtimeCycle: containedRuntimeCycles.length > 0,
          runtimeCycles: containedRuntimeCycles,
          runtimeCycleCount: containedRuntimeCycles.length,
          cycleEdges,
          runtimeCycleEdges,
          componentId,
          topologicalLayer:
            componentId === undefined
              ? undefined
              : layerByComponent.get(componentId),
          outgoingComponents: outgoing,
          outgoingComponentCount: outgoing.length,
          confidence: 'syntactic',
        };
      })
      .sort((a, b) => (a.files[0] ?? '').localeCompare(b.files[0] ?? ''));
    return finalize(
      {
        ...base,
        ...paginateGraphResults(items, query),
        summary: {
          cycleCount: items.length,
          runtimeCycleCount: items.filter(item => item.runtimeCycle).length,
          condensationComponentCount: condensed.components.length,
          condensationEdgeCount: [...condensed.edges.values()].reduce(
            (total, edges) => total + edges.size,
            0
          ),
          topologicalLayerCount: condensed.layers.length,
          transitiveEdgeCount: redundantEdges.size,
        },
        ...(warnings.length > 0 ? { warnings } : {}),
      },
      'Continue cycle components.'
    );
  }

  if (query.operation === 'dependencies' || query.operation === 'dependents') {
    if (!query.file)
      return finalize(
        errorOutput(query, `${query.operation} requires file`),
        `Retry ${query.operation} after expanding the graph scan.`
      );
    const file = normalizeGraphFile(query.file);
    if (!built.fileGraph.has(file)) {
      return finalize(
        errorOutput(query, `file is not in the scanned graph: ${file}`),
        `Retry ${query.operation} after expanding the graph scan.`
      );
    }
    const graph =
      query.operation === 'dependencies'
        ? built.fileGraph
        : reverseGraph(built.fileGraph);
    const condensed = condenseGraph(graph);
    const layerByComponent = componentLayerMap(condensed.layers);
    const redundantEdges = findTransitiveEdges(condensed.edges);
    const immediateDominators = computeImmediateDominators(graph, file);
    const items = traverseGraph(graph, file, query.depth ?? DEFAULT_DEPTH).map(
      result => {
        const resultFile = result.file as string;
        const via = result.via as string;
        const fromComponent = condensed.componentOf.get(via);
        const toComponent = condensed.componentOf.get(resultFile);
        return {
          ...result,
          immediateDominator: immediateDominators.get(resultFile) ?? null,
          topologicalLayer:
            toComponent === undefined
              ? undefined
              : layerByComponent.get(toComponent),
          transitiveEdge:
            fromComponent !== undefined && toComponent !== undefined
              ? redundantEdges.has(`${fromComponent}:${toComponent}`)
              : false,
        };
      }
    );
    return finalize(
      {
        ...base,
        ...paginateGraphResults(items, query),
        summary: {
          source: file,
          depth: query.depth ?? DEFAULT_DEPTH,
          condensationComponentCount: condensed.components.length,
          topologicalLayerCount: condensed.layers.length,
          transitiveEdgeCount: redundantEdges.size,
        },
        ...(warnings.length > 0 ? { warnings } : {}),
      },
      `Continue ${query.operation}.`
    );
  }

  if (query.operation === 'path') {
    if (!query.file || !query.target) {
      return finalize(
        errorOutput(query, 'path requires file and target'),
        'Retry the path query after expanding the graph scan.'
      );
    }
    const file = normalizeGraphFile(query.file);
    const target = normalizeGraphFile(query.target);
    if (!built.fileGraph.has(file) || !built.fileGraph.has(target)) {
      return finalize(
        errorOutput(query, 'file and target must both be in the scanned graph'),
        'Retry the path query after expanding the graph scan.'
      );
    }
    return finalize(
      {
        ...base,
        ...paginateGraphResults(
          [findShortestPath(built.fileGraph, file, target)],
          query
        ),
        summary: { source: file, target },
        ...(warnings.length > 0 ? { warnings } : {}),
      },
      'Continue path results.'
    );
  }

  const knownFiles = new Set(built.facts.keys());
  const resolved = resolveEntrypoints(
    query.path,
    query.entrypoints,
    query.includeTests ?? true,
    knownFiles
  );
  if (resolved.lowConfidence && resolved.entrypoints.length === 0) {
    return finalize(
      {
        ...base,
        status: 'empty',
        results: [],
        summary: {
          ...summarizeEntrypoints(resolved.entrypoints),
          classifiedCount: 0,
          unclassifiedCount: built.fileGraph.size,
        },
        warnings: [...warnings, ...resolved.warnings],
        confidence: 'low',
      },
      'Retry reachability after expanding the graph scan.'
    );
  }
  const reachable = computeReachableFiles(
    built.fileGraph,
    resolved.entrypoints
  );
  const items = [...built.fileGraph.keys()].sort().map(file => ({
    file,
    reachable: reachable.has(file),
    confidence: 'syntactic',
  }));
  return finalize(
    {
      ...base,
      ...paginateGraphResults(items, query),
      summary: {
        ...summarizeEntrypoints(resolved.entrypoints),
        reachableCount: reachable.size,
        unreachableCount: items.length - reachable.size,
      },
      ...(warnings.length + resolved.warnings.length > 0
        ? { warnings: [...warnings, ...resolved.warnings] }
        : {}),
      ...(resolved.lowConfidence ? { confidence: 'low' as const } : {}),
    },
    'Continue reachability classifications.'
  );
}
