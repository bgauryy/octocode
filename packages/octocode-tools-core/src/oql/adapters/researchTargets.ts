/**
 * Research-target adapters: semantics (LSP), repositories, packages,
 * pullRequests, commits, diff, and smart research packets.
 *
 * Each compiles a canonical OQL query (from + scope + `params` bag) into the
 * existing bulk tool runner and maps the single query's `data` payload into
 * generic record rows. Remote semantics route through materialization first
 * (clone → local LSP). This keeps the planner/dispatch uniform; per-target
 * specifics live behind one `params` bag validated by the backing tool.
 *
 * This file is a thin barrel: the actual per-target adapters live under
 * ./researchTargets/* (split to satisfy the repo's max-lines:400 rule). It
 * re-exports every symbol other modules import from here, so nothing outside
 * this directory needs to change its import path.
 */
import {
  executeRepositories,
  executePackages,
} from './researchTargets/repositories.js';
import {
  executeHistory,
  filterCommitsByMatch,
  filterPullRequestsByMatch,
  type PullRequestMatch,
} from './researchTargets/history.js';
import {
  computeLineDiff,
  executeDiff,
  type LineDiff,
} from './researchTargets/diff.js';
import { executeSemantics } from './researchTargets/semantics.js';
import { executeResearch } from './researchTargets/research.js';
import { executeGraph } from './researchTargets/graph.js';
import type { AdapterResult } from './local.js';
import type { OqlQuery } from '../types.js';

export {
  filterCommitsByMatch,
  filterPullRequestsByMatch,
  executeDiff,
  computeLineDiff,
};
export type { PullRequestMatch, LineDiff };

/** Dispatch map: target -> adapter. */
export const RESEARCH_TARGET_ADAPTERS: Record<
  string,
  (q: OqlQuery) => Promise<AdapterResult>
> = {
  repositories: executeRepositories,
  packages: executePackages,
  pullRequests: executeHistory,
  commits: executeHistory,
  diff: executeDiff,
  semantics: executeSemantics,
  research: executeResearch,
  graph: executeGraph,
};
