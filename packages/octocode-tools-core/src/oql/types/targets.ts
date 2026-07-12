/**
 * OQL targets — the addressable "shapes" a query can search
 * (code/content/structure/files/... ) plus reserved (not-yet-active) targets
 * and the sort values each lane can execute.
 */

// Active OQL targets.
export type OqlActiveTarget =
  | 'code'
  | 'content'
  | 'structure'
  | 'files'
  | 'semantics'
  | 'repositories'
  | 'packages'
  | 'pullRequests'
  | 'commits'
  | 'diff'
  | 'research'
  | 'graph'
  // Addressable materialization: clone/cache a bounded corpus and return a
  // stable local checkpoint (not a side-effect of a search).
  | 'materialize';

// Reserved capabilities need proof/dry-run engines before they become targets.
export type OqlReservedTarget = 'fixes' | 'dataflow';

export type OqlTarget = OqlActiveTarget | OqlReservedTarget;

export const ACTIVE_TARGETS: readonly OqlActiveTarget[] = [
  'code',
  'content',
  'structure',
  'files',
  'semantics',
  'repositories',
  'packages',
  'pullRequests',
  'commits',
  'diff',
  'research',
  'graph',
  'materialize',
];

export const RESERVED_TARGETS: readonly OqlReservedTarget[] = [
  'fixes',
  'dataflow',
];

/** Targets that do not need a code corpus (provider/registry discovery). */
export const CORPUS_OPTIONAL_TARGETS: readonly OqlActiveTarget[] = [
  'packages',
  'repositories',
];

/**
 * Which `controls.search.sort` values each lane can actually execute
 * (files: localFindFiles sortBy; code: code-search ranking sorts). Single
 * source for shorthand lowering and the planner's inapplicable-sort warning —
 * a value outside the target's set is IGNORED by the backend, never an error.
 */
export const SEARCH_SORTS_BY_TARGET = {
  code: ['relevance', 'matchCount', 'path', 'modified', 'accessed', 'created'],
  files: ['size', 'name', 'path', 'modified'],
} as const;
