/**
 * OQL query/batch input shapes — the canonical `OqlQuery`/`OqlBatch` a query
 * compiles to, plus the looser `OqlInputQuery`/`OqlInputBatch` edge shapes
 * that accept sugar (repo/owner/ref/text/regex/... shorthand) before
 * normalization.
 */

import type { OqlActiveTarget, OqlTarget } from './targets.js';
import type {
  Predicate,
  QueryScope,
  QuerySource,
  StructuralRule,
} from './predicates.js';

export interface MaterializePolicy {
  mode: 'never' | 'auto' | 'required';
  strategy?: 'file' | 'tree' | 'subtree' | 'repo';
  allowFullRepo?: boolean;
  forceRefresh?: boolean;
}

export interface FetchInstructions {
  content?: {
    range?: { startLine?: number; endLine?: number; contextLines?: number };
    match?: { text: string; regex?: boolean; caseSensitive?: boolean };
    contentView?: 'none' | 'standard' | 'symbols';
    charOffset?: number;
    charLength?: number;
    fullContent?: boolean;
  };
  tree?: {
    maxDepth?: number;
    pattern?: string;
    includeSizes?: boolean;
    extensions?: string[];
    filesOnly?: boolean;
    directoriesOnly?: boolean;
    sortBy?: 'name' | 'size' | 'time' | 'extension';
    reverse?: boolean;
  };
}

export interface QueryControls {
  search?: {
    countLinesPerFile?: boolean;
    countMatchesPerFile?: boolean;
    onlyMatching?: boolean;
    unique?: boolean;
    countUnique?: boolean;
    contextLines?: number;
    invertMatch?: boolean;
    matchWindow?: number;
    matchContentLength?: number;
    maxMatchesPerFile?: number;
    matchPage?: number;
    // 'size' and 'name' apply to target:"files" only (lowered to
    // localFindFiles sortBy); the rest are code-search sorts.
    sort?:
      | 'relevance'
      | 'matchCount'
      | 'path'
      | 'modified'
      | 'accessed'
      | 'created'
      | 'size'
      | 'name';
    sortReverse?: boolean;
    rankingProfile?: string;
    debugRanking?: boolean;
  };
  budget?: {
    maxFiles?: number;
    maxCandidates?: number;
    maxBytes?: number;
    maxMaterializedBytes?: number;
    maxPlanNodes?: number;
    maxBooleanExpansion?: number;
    timeoutMs?: number;
  };
}

export type SelectField = string;

export type QueryView = 'discovery' | 'paginated' | 'detailed';

export interface OqlQuery {
  schema: 'oql';
  id?: string;
  target: OqlActiveTarget;
  from?: QuerySource;
  scope?: QueryScope;
  where?: Predicate;
  materialize?: MaterializePolicy;
  fetch?: FetchInstructions;
  select?: SelectField[];
  view?: QueryView;
  controls?: QueryControls;
  limit?: number;
  page?: number;
  itemsPerPage?: number;
  /**
   * Target-specific parameter bag. The backing tool's schema remains the
   * exhaustive validator; OQL validates the documented common fields early.
   */
  params?: Record<string, unknown>;
  explain?: boolean;
}

export interface OqlBatch {
  schema: 'oql';
  id?: string;
  queries: OqlQuery[];
  combine?: 'independent' | 'merge';
  limit?: number;
  page?: number;
  itemsPerPage?: number;
  explain?: boolean;
}

export type OqlCanonicalInput = OqlQuery | OqlBatch;

export interface OqlInputQuery {
  schema?: 'oql';
  id?: string;
  target: OqlTarget;
  from?: QuerySource;
  scope?: QueryScope;
  where?: Predicate;
  materialize?: MaterializePolicy | 'never' | 'auto' | 'required';
  fetch?: FetchInstructions;
  select?: SelectField[];
  view?: QueryView;
  controls?: QueryControls;
  limit?: number;
  page?: number;
  itemsPerPage?: number;
  explain?: boolean;
  // sugar accepted only at the edge
  repo?: string;
  owner?: string;
  ref?: string;
  path?: string | string[];
  text?: string;
  regex?: string;
  pattern?: string;
  rule?: StructuralRule;
  lang?: string;
  [key: string]: unknown;
}

export interface OqlInputBatch {
  schema?: 'oql';
  id?: string;
  queries: OqlInputQuery[];
  combine?: 'independent' | 'merge';
  limit?: number;
  page?: number;
  itemsPerPage?: number;
  explain?: boolean;
}

export type OqlSearchInput = OqlInputQuery | OqlInputBatch;
