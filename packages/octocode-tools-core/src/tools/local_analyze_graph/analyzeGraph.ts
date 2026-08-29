import { LSP_GET_SEMANTICS_TOOL_NAME } from '../toolNames.js';
import {
  buildFileGraph,
  DEFAULT_DEAD_CODE_EXCLUDE_DIRS,
  type WalkResult,
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

export type GraphOperation =
  | 'deadCode'
  | 'cycles'
  | 'dependencies'
  | 'dependents'
  | 'path'
  | 'reachability';

export interface AnalyzeGraphQuery {
  operation: GraphOperation;
  path: string;
  file?: string;
  target?: string;
  depth?: number;
  entrypoints?: string[];
  includeTests?: boolean;
  excludeDir?: string[];
  maxFiles?: number;
  limit?: number;
  page?: number;
  itemsPerPage?: number;
}

export interface AnalyzeGraphOutput {
  status?: 'empty' | 'error';
  error?: string;
  errorCode?: string;
  operation: GraphOperation;
  path: string;
  filesScanned?: number;
  filesSkipped?: number;
  results: Array<Record<string, unknown>>;
  summary?: Record<string, unknown>;
  pagination?: {
    currentPage: number;
    totalPages: number;
    entriesPerPage: number;
    totalEntries: number;
    hasMore: boolean;
    outOfRange?: boolean;
  };
  next?: Record<string, unknown>;
  warnings?: string[];
  confidence?: 'low';
  [key: string]: unknown;
}

export interface AnalyzeGraphContext {
  getGraph?: (
    path: string,
    excludeDir: string[],
    maxFiles: number
  ) => WalkResult | Promise<WalkResult>;
}

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
  const excludeDir =
    query.excludeDir && query.excludeDir.length > 0
      ? query.excludeDir
      : DEFAULT_DEAD_CODE_EXCLUDE_DIRS;
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
    const items = findStronglyConnectedComponents(built.fileGraph)
      .map(component => {
        const allFiles = [...component.files].sort();
        return {
          files: allFiles.slice(0, MAX_FILES_PER_COMPONENT),
          size: allFiles.length,
          edgeKinds: collectGraphEdgeKinds(built.fileGraph, allFiles),
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
        summary: { cycleCount: items.length },
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
    const items = traverseGraph(graph, file, query.depth ?? DEFAULT_DEPTH);
    return addNextSteps(
      {
        ...base,
        ...paginate(items, query),
        summary: { source: file, depth: query.depth ?? DEFAULT_DEPTH },
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
