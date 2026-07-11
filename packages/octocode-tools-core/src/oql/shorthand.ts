/**
 * Shorthand lowering for `octocode search`.
 *
 * The CLI reads argv and resolves a target string to a corpus (local path vs
 * GitHub ref — the only step that needs the filesystem). Everything else —
 * which predicate the flags select, dialect, lang requirements, and assembling
 * the sugar object the normalizer accepts — lives here so it is owned once in
 * tools-core (not re-implemented in the interface) and is unit-testable without
 * argv or a terminal.
 *
 * This file is the orchestrator/barrel: target resolution and source/scope
 * assembly live here; the field-level builders live in ./shorthand/*.
 */
import {
  ACTIVE_TARGETS,
  CORPUS_OPTIONAL_TARGETS,
  type OqlActiveTarget,
  type OqlInputQuery,
  type QueryScope,
  type QuerySource,
} from './types.js';
import { buildPredicate } from './shorthand/predicates.js';
import {
  fetchInstructions,
  hasContentFetch,
  materializePolicy,
  queryControls,
  resolveView,
} from './shorthand/query-controls.js';
import { targetParams } from './shorthand/target-params.js';
import type {
  SearchShorthand,
  ShorthandCorpus,
  ShorthandResult,
} from './shorthand/types.js';

export type { SearchShorthand, ShorthandCorpus, ShorthandResult };

/**
 * Lower shorthand parts into the OQL sugar object. Predicate precedence:
 * pattern > rule > regex > text. Returns a typed error for invalid combos
 * (e.g. structural without `lang`) instead of throwing.
 */
export function buildShorthandInput(parts: SearchShorthand): ShorthandResult {
  const targetResult = resolveTarget(parts);
  if ('error' in targetResult) return targetResult;
  const target = targetResult.target;

  if (parts.search === 'both') {
    const pathQuery = buildSingleQuery({
      ...parts,
      target: 'files',
      search: 'path',
    });
    if ('error' in pathQuery) return pathQuery;
    const contentQuery = buildSingleQuery({
      ...parts,
      target: target === 'files' ? 'code' : target,
      search: 'content',
    });
    if ('error' in contentQuery) return contentQuery;
    return {
      input: {
        schema: 'oql',
        queries: [pathQuery.query, contentQuery.query],
        combine: 'independent',
        ...(parts.limit !== undefined ? { limit: parts.limit } : {}),
        ...(parts.page !== undefined ? { page: parts.page } : {}),
        ...(parts.itemsPerPage !== undefined
          ? { itemsPerPage: parts.itemsPerPage }
          : {}),
      },
    };
  }

  const built = buildSingleQuery({ ...parts, target });
  return 'error' in built ? built : { input: built.query };
}

function buildSingleQuery(
  parts: SearchShorthand & { target: OqlActiveTarget }
): { query: OqlInputQuery } | { error: string } {
  const view = resolveView(parts.view);
  if ('error' in view) return view;

  const sourceScope = sourceAndScope(parts);
  const targetUsesWhere = parts.target === 'code' || parts.target === 'files';
  const whereResult = targetUsesWhere ? buildPredicate(parts) : {};
  if ('error' in whereResult) return whereResult;

  const targetNeedsPredicate = ['code', 'files'].includes(parts.target);
  if (
    targetNeedsPredicate &&
    whereResult.where === undefined &&
    !parts.filesOnly &&
    !parts.filesWithoutMatch
  ) {
    return {
      error: 'No search term: provide text, --regex, --pattern, or --rule.',
    };
  }

  const query: OqlInputQuery = {
    schema: 'oql',
    target: parts.target,
    ...sourceScope,
  };

  if (whereResult.where) query.where = whereResult.where;
  if (view.view) query.view = view.view;
  if (parts.limit !== undefined) query.limit = parts.limit;
  if (parts.page !== undefined) query.page = parts.page;
  if (parts.itemsPerPage !== undefined) query.itemsPerPage = parts.itemsPerPage;

  const materialize = materializePolicy(parts);
  if (materialize) query.materialize = materialize;

  const fetch = fetchInstructions(parts);
  if (fetch) query.fetch = fetch;

  const controls = queryControls(parts);
  if (controls) query.controls = controls;

  const params = targetParams(parts);
  if (Object.keys(params).length > 0) query.params = params;

  return { query };
}

function resolveTarget(
  parts: SearchShorthand
): { target: OqlActiveTarget } | { error: string } {
  const explicit = parts.target;
  if (explicit) {
    if (isActiveTarget(explicit)) return { target: explicit };
    return {
      error: `--target must be one of: ${ACTIVE_TARGETS.join(', ')}.`,
    };
  }

  if (parts.op || parts.symbol || parts.uri || parts.workspaceRoot)
    return { target: 'semantics' };
  if (parts.tree) return { target: 'structure' };
  if (hasContentFetch(parts)) return { target: 'content' };
  if (parts.search === 'path') return { target: 'files' };
  if (parts.corpus.kind === 'npm') return { target: 'packages' };

  return { target: 'code' };
}

function isActiveTarget(value: string): value is OqlActiveTarget {
  return (ACTIVE_TARGETS as readonly string[]).includes(value);
}

function sourceAndScope(parts: SearchShorthand): {
  from?: QuerySource;
  scope?: QueryScope;
} {
  const scope: QueryScope = {};
  if (parts.lang) scope.language = parts.lang;
  if (parts.include?.length) scope.include = parts.include;
  if (parts.excludeDir?.length) scope.excludeDir = parts.excludeDir;
  if (parts.extension) {
    const ext = parts.extension.replace(/^\./, '');
    scope.include = [...(scope.include ?? []), `**/*.${ext}`];
  }
  if (parts.exclude?.length) scope.exclude = parts.exclude;
  if (parts.hidden) scope.hidden = true;
  if (parts.noIgnore) scope.noIgnore = true;
  if (parts.minDepth !== undefined) scope.minDepth = parts.minDepth;
  if (parts.maxDepth !== undefined) scope.maxDepth = parts.maxDepth;

  if (
    CORPUS_OPTIONAL_TARGETS.includes(parts.target as OqlActiveTarget) &&
    parts.corpus.kind === 'local'
  ) {
    return Object.keys(scope).length > 0 ? { scope } : {};
  }

  if (parts.corpus.kind === 'npm') {
    return {
      from: { kind: 'npm' },
      ...(Object.keys(scope).length > 0 ? { scope } : {}),
    };
  }

  if (parts.corpus.kind === 'github') {
    const from: QuerySource = {
      kind: 'github',
      repo: parts.corpus.repo,
      ...(parts.branch || parts.corpus.ref
        ? { ref: parts.branch ?? parts.corpus.ref }
        : {}),
    };
    if (parts.corpus.path) scope.path = parts.corpus.path;
    return {
      from,
      ...(Object.keys(scope).length > 0 ? { scope } : {}),
    };
  }

  return {
    from: { kind: 'local', path: parts.corpus.path },
    ...(Object.keys(scope).length > 0 ? { scope } : {}),
  };
}
