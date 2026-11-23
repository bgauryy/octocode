import { octocode } from './runtime/lib.ts';

import type {
  GraphBuildInput,
  GraphBuildOutput,
  GraphQueryInput,
  GraphQueryOutput,
} from './contract.generated.js';
export type {
  GraphBuildInput,
  GraphBuildOutput,
  GraphQueryInput,
  GraphQueryOutput,
} from './contract.generated.js';
export { startMcp } from './mcp.ts';
// Saved graphs: build one with the native engine, refresh it after edits, open it and ask it questions, diff two.
export {
  buildGraph,
  openGraph,
  previousGraph,
  refreshGraph,
} from './runtime/api.ts';
export type {
  BuildOptions,
  BuildSummary,
  RefreshOptions,
  RefreshSummary,
  SavedGraph,
} from './runtime/api.ts';
export { formatResult, runQuery } from './runtime/query-api.ts';
export type { QueryData, QueryResult } from './runtime/query-api.ts';
export { diffGraphs, formatDiff } from './runtime/diff.ts';
export type { GraphDiff } from './runtime/diff.ts';
export {
  LinkRefused,
  proposeLink,
  readProposed,
  removeLink,
  reviewLink,
} from './runtime/links-proposed.ts';
export type {
  Anchor,
  LinkProposal,
  ProposedLink,
  Refusal,
} from './runtime/links-proposed.ts';
export type { Link, UnresolvedLink } from './runtime/types.ts';

/** The native engine validates the input and preserves the full response envelope. */
export function graphBuild(input: GraphBuildInput): Promise<GraphBuildOutput> {
  return octocode(['graphBuild', '--json', JSON.stringify(input)], {
    partial: true,
  }) as Promise<GraphBuildOutput>;
}

export function graphQuery(input: GraphQueryInput): Promise<GraphQueryOutput> {
  return octocode(['graphQuery', '--json', JSON.stringify(input)], {
    partial: true,
  }) as Promise<GraphQueryOutput>;
}
