import {
  buildFileGraph,
  type WalkResult,
} from '../../../graph/buildFileGraph.js';
import { findStronglyConnectedComponents } from '../../../graph/reachability.js';
import type {
  TopologyAnalysisContext,
  TopologyAnalysisOutput,
  TopologyAnalysisQuery,
} from './analysisTypes.js';
import { paginateGraphResults } from './pagination.js';

type CompleteQuery = TopologyAnalysisQuery & { path: string };
type Finalize = (
  output: TopologyAnalysisOutput,
  why: string
) => TopologyAnalysisOutput;

export async function analyzeTopologyDrift(
  head: WalkResult,
  query: CompleteQuery,
  context: TopologyAnalysisContext,
  excludeDir: string[],
  maxFiles: number,
  finalize: Finalize
): Promise<TopologyAnalysisOutput> {
  if (!query.baseline) {
    return finalize(
      errorOutput(query, 'drift requires baseline'),
      'Retry drift with a baseline root.'
    );
  }
  const baseline = await (context.getGraph?.(
    query.baseline,
    excludeDir,
    maxFiles,
    query.rustWorkspace
  ) ??
    buildFileGraph(query.baseline, excludeDir, maxFiles, query.rustWorkspace));
  const headFiles = new Set(head.fileGraph.keys());
  const baseFiles = new Set(baseline.fileGraph.keys());
  const headEdges = graphEdges(head);
  const baseEdges = graphEdges(baseline);
  const results: Array<Record<string, unknown>> = [];
  for (const edge of [...headEdges.keys()]
    .filter(key => !baseEdges.has(key))
    .sort()) {
    results.push({
      category: 'relation',
      change: 'added',
      ...headEdges.get(edge),
      confidence: 'syntactic',
    });
  }
  for (const edge of [...baseEdges.keys()]
    .filter(key => !headEdges.has(key))
    .sort()) {
    results.push({
      category: 'relation',
      change: 'removed',
      ...baseEdges.get(edge),
      confidence: 'syntactic',
    });
  }
  const headCycles = cycleKeys(head);
  const baseCycles = cycleKeys(baseline);
  for (const key of [...headCycles.keys()]
    .filter(key => !baseCycles.has(key))
    .sort()) {
    results.push({
      category: 'cycle',
      change: 'added',
      files: headCycles.get(key),
      confidence: 'syntactic',
    });
  }
  for (const key of [...baseCycles.keys()]
    .filter(key => !headCycles.has(key))
    .sort()) {
    results.push({
      category: 'cycle',
      change: 'resolved',
      files: baseCycles.get(key),
      confidence: 'syntactic',
    });
  }
  for (const file of [...headFiles]
    .filter(file => !baseFiles.has(file))
    .sort()) {
    results.push({ category: 'file', change: 'added', file });
  }
  for (const file of [...baseFiles]
    .filter(file => !headFiles.has(file))
    .sort()) {
    results.push({ category: 'file', change: 'removed', file });
  }
  const page = paginateGraphResults(results, query);
  return finalize(
    {
      operation: 'drift',
      path: query.path,
      baseline: query.baseline,
      filesScanned: head.filesScanned,
      baselineFilesScanned: baseline.filesScanned,
      ...page,
      summary: {
        comparable: true,
        relationsAdded: count(results, 'relation', 'added'),
        relationsRemoved: count(results, 'relation', 'removed'),
        cyclesAdded: count(results, 'cycle', 'added'),
        cyclesResolved: count(results, 'cycle', 'resolved'),
        filesAdded: count(results, 'file', 'added'),
        filesRemoved: count(results, 'file', 'removed'),
      },
      ...(head.truncated || baseline.truncated
        ? {
            truncated: true,
            partialReasons: ['maxFiles' as const],
            warnings: [
              'one or both graph scans reached maxFiles; drift is partial',
            ],
          }
        : {}),
    },
    'Continue topology drift results.'
  );
}

function count(
  results: Array<Record<string, unknown>>,
  category: string,
  change: string
): number {
  return results.filter(
    row => row.category === category && row.change === change
  ).length;
}

function errorOutput(
  query: CompleteQuery,
  message: string
): TopologyAnalysisOutput {
  return {
    status: 'error',
    error: message,
    errorCode: 'invalidGraphQuery',
    operation: query.operation,
    path: query.path,
    results: [],
  };
}

function graphEdges(
  built: WalkResult
): Map<string, { from: string; to: string; edgeKind: string }> {
  const edges = new Map<
    string,
    { from: string; to: string; edgeKind: string }
  >();
  for (const [from, node] of built.fileGraph) {
    for (const to of node.importsFiles) {
      const kinds =
        node.edgeKinds.get(to) ?? new Set(['static-import' as const]);
      for (const edgeKind of kinds) {
        edges.set(`${from}\0${edgeKind}\0${to}`, { from, to, edgeKind });
      }
    }
  }
  return edges;
}

function cycleKeys(built: WalkResult): Map<string, string[]> {
  const cycles = new Map<string, string[]>();
  for (const component of findStronglyConnectedComponents(built.fileGraph)) {
    const files = [...component.files].sort();
    if (
      files.length > 1 ||
      built.fileGraph.get(files[0] ?? '')?.importsFiles.has(files[0] ?? '')
    ) {
      cycles.set(files.join('\0'), files);
    }
  }
  return cycles;
}
