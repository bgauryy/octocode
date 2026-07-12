/**
 * OQL record `data` contracts — the documented payload shapes per
 * `recordType` (repository, package, PR, commit, diff, semantics,
 * materialized, research, graph). The backing tool owns the exhaustive
 * payload; these name the fields agents rely on to cite + continue. All
 * optional (backend-dependent); never fabricated.
 */

import type { Pagination } from './envelope.js';

export interface OqlRepositoryData {
  fullName?: string;
  owner?: string;
  repo?: string;
  description?: string;
  stars?: number;
  forks?: number;
  language?: string;
  topics?: string[];
  pushedAt?: string;
  url?: string;
  [k: string]: unknown;
}
export interface OqlPackageData {
  name?: string;
  version?: string;
  description?: string;
  downloads?: number;
  repository?: string;
  repositoryId?: string;
  [k: string]: unknown;
}
export interface OqlPullRequestData {
  number?: number;
  title?: string;
  state?: string;
  author?: string;
  createdAt?: string;
  mergedAt?: string;
  changedFiles?: number;
  url?: string;
  [k: string]: unknown;
}
export interface OqlCommitData {
  sha?: string;
  oid?: string;
  message?: string;
  title?: string;
  author?: string;
  date?: string;
  [k: string]: unknown;
}
export interface OqlDiffData {
  path?: string;
  baseRef?: string;
  headRef?: string;
  additions?: number;
  deletions?: number;
  unchanged?: number;
  patch?: string;
  [k: string]: unknown;
}
export interface OqlSemanticsData {
  uri?: string;
  line?: number;
  startLine?: number;
  symbol?: string;
  kind?: string;
  [k: string]: unknown;
}
export interface OqlMaterializedData {
  localPath: string;
  repoRoot?: string;
  ref?: string;
  cache?: 'hit' | 'miss';
  complete?: boolean;
  [k: string]: unknown;
}
export interface OqlResearchData {
  kind?: 'researchFlow';
  goal?: string;
  intent?: string;
  facets?: readonly string[];
  mode?: 'plan' | 'analyze' | 'prove';
  summary?: Record<string, unknown>;
  flow?: readonly unknown[];
  nativeGraphSummary?: Record<string, unknown>;
  graphSummary?: Record<string, unknown>;
  packetPage?: Pagination;
  packets?: unknown[];
  /** Present only in detailed view — a windowed slice (see `manifestsPage`). */
  manifests?: unknown[];
  manifestsPage?: Pagination;
  /** Present only in detailed view — a windowed slice (see `filesPage`). */
  files?: unknown[];
  filesPage?: Pagination;
  /** Present only in detailed view — a windowed slice (see `dependenciesPage`). */
  dependencies?: unknown[];
  dependenciesPage?: Pagination;
  /** Present only in detailed view — a windowed slice (see `symbolsPage`). */
  symbols?: unknown[];
  symbolsPage?: Pagination;
  /** Present only in detailed view — a windowed slice (see `graphFactsPage`). */
  graphFacts?: unknown[];
  graphFactsPage?: Pagination;
  caveats?: string[];
  [k: string]: unknown;
}
export interface OqlGraphData {
  kind?: 'relationshipGraph';
  goal?: string;
  intent?: string;
  facets?: readonly string[];
  mode?: 'plan' | 'analyze' | 'prove';
  root?: string;
  filters?: Record<string, unknown>;
  summary?: Record<string, unknown>;
  flow?: readonly unknown[];
  nativeGraphSummary?: Record<string, unknown>;
  graphSummary?: unknown;
  packetPage?: Pagination;
  nodes?: unknown[];
  edges?: unknown[];
  facts?: unknown[];
  packets?: unknown[];
  missingProof?: unknown[];
  caveats?: string[];
  [k: string]: unknown;
}
