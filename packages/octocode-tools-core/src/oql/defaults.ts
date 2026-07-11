/**
 * OQL defaults (see OCTOCODE_QUERY_LANGUAGE.md §defaults). `--explain` must
 * surface every applied default, so they live in one place.
 */
import type { OqlQuery } from './types.js';

export const DEFAULTS = {
  schema: 'oql' as const,
  view: 'paginated' as const,
  page: 1,
  itemsPerPage: 25,
  githubMaterializeMode: 'never' as const,
  textCase: 'smart' as const,
  regexDialectLocal: 'rust' as const,
  regexCase: 'smart' as const,
  contentView: 'standard' as const,
  contentCharLength: 20000,
  matchContentLength: 500,
  maxPlanNodes: 128,
  maxBooleanExpansion: 64,
  normalCodeContext: 2,
  detailedCodeContext: 3,
  localSearchSort: 'relevance' as const,
  localRankingProfile: 'auto' as const,
};

/** The subset of defaults actually applied to (or relevant for) this query. */
export function appliedDefaults(query: OqlQuery): Record<string, unknown> {
  const applied: Record<string, unknown> = {
    schema: DEFAULTS.schema,
    view: query.view ?? DEFAULTS.view,
    page: query.page ?? DEFAULTS.page,
    itemsPerPage: query.itemsPerPage ?? DEFAULTS.itemsPerPage,
    maxPlanNodes: query.controls?.budget?.maxPlanNodes ?? DEFAULTS.maxPlanNodes,
  };
  if (query.from?.kind === 'github') {
    applied['materialize.mode'] =
      query.materialize?.mode ?? DEFAULTS.githubMaterializeMode;
  }
  if (query.target === 'content') {
    applied['fetch.content.contentView'] =
      query.fetch?.content?.contentView ?? DEFAULTS.contentView;
  }
  if (query.target === 'code') {
    applied.codeContext =
      query.view === 'detailed'
        ? DEFAULTS.detailedCodeContext
        : DEFAULTS.normalCodeContext;
    applied['search.sort'] =
      query.controls?.search?.sort ?? DEFAULTS.localSearchSort;
  }
  return applied;
}
