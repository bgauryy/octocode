import { LSP_GET_SEMANTICS_TOOL_NAME } from '../toolNames.js';
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

const DEFAULT_ITEMS_PER_PAGE = 50;
const DEFAULT_MAX_FILES = 20_000;
const DEFAULT_DEPTH = 1;
const MAX_FILES_PER_COMPONENT = 50;
const MAX_ENTRYPOINTS_IN_SUMMARY = 50;

function summarizeEntrypoints(entrypoints: string[]): Record<string, unknown> {
  return {
    entrypointsResolved: entrypoints.slice(0, MAX_ENTRYPOINTS_IN_SUMMARY),
    entrypointsResolvedCount: entrypoints.length,
    ...(entrypoints.length > MAX_ENTRYPOINTS_IN_SUMMARY
      ? { entrypointsResolvedTruncated: true }
      : {}),
  };
}

function paginate(
  items: Array<Record<string, unknown>>,
  query: AnalyzeGraphQuery
): Pick<AnalyzeGraphOutput, 'results' | 'pagination'> {
  const limited = query.limit ? items.slice(0, query.limit) : items;
  const itemsPerPage = query.itemsPerPage ?? DEFAULT_ITEMS_PER_PAGE;
  const requestedPage = query.page ?? 1;
  const totalPages = Math.max(1, Math.ceil(limited.length / itemsPerPage));
  const currentPage = Math.min(Math.max(1, requestedPage), totalPages);
  const start = (currentPage - 1) * itemsPerPage;
  return {
    results: limited.slice(start, start + itemsPerPage),
    pagination: {
      currentPage,
      totalPages,
      entriesPerPage: itemsPerPage,
      totalEntries: limited.length,
      hasMore: currentPage < totalPages,
      ...(requestedPage > totalPages ? { outOfRange: true } : {}),
    },
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

function addNextSteps(
  output: AnalyzeGraphOutput,
  query: AnalyzeGraphQuery,
  why: string
): AnalyzeGraphOutput {
  if (output.pagination?.outOfRange) {
    output = {
      ...output,
      warnings: [
        ...(output.warnings ?? []),
        `page:${query.page} is out of range (only ${output.pagination.totalPages} page(s)) — returned page ${output.pagination.currentPage} instead.`,
      ],
    };
  }
  const next: Record<string, unknown> = {};
  if (output.pagination?.hasMore) {
    next.nextPage = {
      tool: 'localAnalyzeGraph',
      query: { ...query, page: output.pagination.currentPage + 1 },
      why,
      confidence: 'exact',
    };
  }

  if (query.operation === 'deadCode') {
    const candidate = output.results[0];
    if (
      candidate &&
      typeof candidate.file === 'string' &&
      typeof candidate.name === 'string' &&
      typeof candidate.line === 'number'
    ) {
      const root = query.path.replace(/\/+$/, '');
      next.verifyReferences = {
        tool: LSP_GET_SEMANTICS_TOOL_NAME,
        query: {
          type: 'references',
          uri: `${root}/${candidate.file}`,
          symbolName: candidate.name,
          lineHint: candidate.line,
          includeDeclaration: false,
          groupByFile: true,
        },
        why: `Verify candidate "${candidate.name}" before deletion; repeat for each result, prioritizing viaHeuristic:"reexport-chain".`,
        confidence: 'high',
      };
    }
  }

  return Object.keys(next).length > 0 ? { ...output, next } : output;
}

export async function analyzeGraph(
  query: AnalyzeGraphQuery,
  context: AnalyzeGraphContext = {}
): Promise<AnalyzeGraphOutput> {
  const excludeDir = resolveGraphExcludeDirs(query.excludeDir);
  const maxFiles = query.maxFiles ?? DEFAULT_MAX_FILES;
  const built = await (context.getGraph?.(query.path, excludeDir, maxFiles) ??
    buildFileGraph(query.path, excludeDir, maxFiles));

  if (query.operation === 'deadCode') {
    const scan = scanForDeadCode(query.path, query, built);
    const page = paginate(
      scan.deadExports as unknown as Array<Record<string, unknown>>,
      query
    );
    const pageClusterIds = new Set(
      page.results
        .map(result => result.clusterId)
        .filter((id): id is number => typeof id === 'number')
    );
    return addNextSteps(
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
              files: cluster.files.slice(0, MAX_FILES_PER_COMPONENT),
              size: cluster.files.length,
              edgeKinds: collectGraphEdgeKinds(built.fileGraph, cluster.files),
              confidence: 'syntactic',
              ...(cluster.files.length > MAX_FILES_PER_COMPONENT
                ? { truncated: true }
                : {}),
            })),
          deadClusterCount: scan.deadClusters.length,
          deadExportCount: scan.deadExports.length,
        },
        ...(scan.warnings.length > 0 ? { warnings: scan.warnings } : {}),
        ...(scan.confidence ? { confidence: scan.confidence } : {}),
      },
      query,
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
          files: allFiles.slice(0, MAX_FILES_PER_COMPONENT),
          size: allFiles.length,
          edgeKinds: collectGraphEdgeKinds(built.fileGraph, allFiles),
          runtimeCycle: containedRuntimeCycles.length > 0,
          runtimeCycles: containedRuntimeCycles.slice(0, 10),
          runtimeCycleCount: containedRuntimeCycles.length,
          cycleEdges,
          runtimeCycleEdges,
          componentId,
          topologicalLayer:
            componentId === undefined
              ? undefined
              : layerByComponent.get(componentId),
          outgoingComponents: outgoing.slice(0, MAX_FILES_PER_COMPONENT),
          outgoingComponentCount: outgoing.length,
          ...(outgoing.length > MAX_FILES_PER_COMPONENT
            ? { outgoingComponentsTruncated: true }
            : {}),
          ...(allFiles.length > MAX_FILES_PER_COMPONENT
            ? { truncated: true }
            : {}),
          confidence: 'syntactic',
        };
      })
      .sort((a, b) => (a.files[0] ?? '').localeCompare(b.files[0] ?? ''));
    return addNextSteps(
      {
        ...base,
        ...paginate(items, query),
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
      query,
      'Continue cycle components.'
    );
  }

  if (query.operation === 'dependencies' || query.operation === 'dependents') {
    if (!query.file)
      return errorOutput(query, `${query.operation} requires file`);
    const file = normalizeGraphFile(query.file);
    if (!built.fileGraph.has(file)) {
      return errorOutput(query, `file is not in the scanned graph: ${file}`);
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
    return addNextSteps(
      {
        ...base,
        ...paginate(items, query),
        summary: {
          source: file,
          depth: query.depth ?? DEFAULT_DEPTH,
          condensationComponentCount: condensed.components.length,
          topologicalLayerCount: condensed.layers.length,
          transitiveEdgeCount: redundantEdges.size,
        },
        ...(warnings.length > 0 ? { warnings } : {}),
      },
      query,
      `Continue ${query.operation}.`
    );
  }

  if (query.operation === 'path') {
    if (!query.file || !query.target) {
      return errorOutput(query, 'path requires file and target');
    }
    const file = normalizeGraphFile(query.file);
    const target = normalizeGraphFile(query.target);
    if (!built.fileGraph.has(file) || !built.fileGraph.has(target)) {
      return errorOutput(
        query,
        'file and target must both be in the scanned graph'
      );
    }
    return addNextSteps(
      {
        ...base,
        ...paginate([findShortestPath(built.fileGraph, file, target)], query),
        summary: { source: file, target },
        ...(warnings.length > 0 ? { warnings } : {}),
      },
      query,
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
  const reachable = computeReachableFiles(
    built.fileGraph,
    resolved.entrypoints
  );
  const items = [...built.fileGraph.keys()].sort().map(file => ({
    file,
    reachable: reachable.has(file),
    confidence: 'syntactic',
  }));
  return addNextSteps(
    {
      ...base,
      ...paginate(items, query),
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
    query,
    'Continue reachability classifications.'
  );
}
