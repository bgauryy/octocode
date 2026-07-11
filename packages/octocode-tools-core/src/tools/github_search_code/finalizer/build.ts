import type { BulkFinalizer } from '../../../types/bulk.js';
import {
  collectFlatErrors,
  formatFinalizedResponse,
  type QueryWithPagination,
} from '../../../utils/response/groupedFinalizer.js';
import type { GitHubCodeSearchOutputLocal } from '../scheme.js';
import type { RepoState } from '../execution.js';
import type { ToolDiagnostic } from '../../../scheme/pagination.js';
import { type CodeSearchPagination } from '../../providerMappers.js';
import {
  applyExactMatchRanking,
  hasScopedGitHubQuery,
  queryById,
  readPerQueryFlat,
} from './ranking.js';
import {
  buildNextMap,
  buildResultRecords,
  mergeGroups,
  type PerQueryGroups,
} from './groups.js';

export function buildGhSearchCodeFinalizer<
  TQuery extends QueryWithPagination,
>(): BulkFinalizer<TQuery, GitHubCodeSearchOutputLocal> {
  return ({ queries, results }) => {
    const perQueryGroups: PerQueryGroups[] = [];
    const paginationByQuery = new Map<string, CodeSearchPagination>();

    const emptyQueries: Array<{
      id: string;
      nonExistentScope?: true;
      incompleteResults?: true;
    }> = [];
    let anyIncompleteResults = false;

    const repoStates: Array<{
      id: string;
      state: RepoState;
      query: QueryWithPagination | undefined;
    }> = [];
    const queriesById = queryById(queries);

    results.forEach((res, _index) => {
      if (res.status === 'error') return;

      const flat = readPerQueryFlat(res);
      if (flat.repoState) {
        repoStates.push({
          id: res.id,
          state: flat.repoState,
          query: queriesById.get(res.id),
        });
      }
      if (flat.incompleteResults) anyIncompleteResults = true;
      const totalMatches = flat.results.reduce(
        (sum, group) => sum + group.matches.length,
        0
      );
      if (totalMatches === 0) {
        emptyQueries.push({
          id: res.id,
          ...(flat.nonExistentScope ? { nonExistentScope: true as const } : {}),
          ...(flat.incompleteResults
            ? { incompleteResults: true as const }
            : {}),
        });
      }
      const groups = flat.results;
      perQueryGroups.push({ id: res.id, groups });

      if (flat.pagination) {
        paginationByQuery.set(res.id, flat.pagination);
      }
    });

    const allKeywords = Array.from(
      new Set(
        queries.flatMap(q => {
          const kws = (q as { keywords?: unknown }).keywords;
          return Array.isArray(kws)
            ? kws.filter((k): k is string => typeof k === 'string')
            : [];
        })
      )
    );
    const groups = applyExactMatchRanking(
      mergeGroups(perQueryGroups),
      allKeywords
    );

    const errors = collectFlatErrors(results);
    const conciseMode = queries.some(
      q => (q as { concise?: boolean }).concise === true
    );
    const resultRecords = buildResultRecords(
      queries,
      groups,
      paginationByQuery
    );
    const nextMap = buildNextMap(resultRecords, queries, allKeywords);
    if (conciseMode) {
      for (const rec of resultRecords) {
        rec.data.files = rec.data.files.map(
          f => `${f.owner}/${f.repo}:${f.path}`
        ) as unknown as typeof rec.data.files;
      }
    }
    const responseData: GitHubCodeSearchOutputLocal = {
      results: resultRecords,
      ...(nextMap ? { next: nextMap } : {}),
    };

    if (emptyQueries.length > 0) {
      responseData.emptyQueries = emptyQueries.map(
        ({ id, nonExistentScope, incompleteResults }) => ({
          id,
          ...(nonExistentScope ? { nonExistentScope } : {}),
          ...(incompleteResults ? { incompleteResults } : {}),
        })
      );
    }
    if (errors.length > 0) responseData.errors = errors;

    // Every advisory goes out twice on purpose: `warnings` is the rendered
    // string, `diagnostics` the machine-routable duplicate with a stable code.
    const diagnostics: ToolDiagnostic[] = [];
    const warn = (
      code: string,
      message: string,
      level: ToolDiagnostic['level'] = 'warning'
    ): void => {
      responseData.warnings = [...(responseData.warnings ?? []), message];
      diagnostics.push({ level, code, message });
    };

    // GitHub's index did not fully complete for at least one query — empty or
    // partial results may be a false negative, NOT a true absence.
    if (anyIncompleteResults) {
      warn(
        'ghIncompleteResults',
        'GitHub code search returned incomplete_results: the search index did not fully complete. Empty or partial results may be a false negative — retry, narrow scope (owner/repo/path), or materialize the repo and search locally before concluding absence.'
      );
    }

    if (
      emptyQueries.length > 0 &&
      hasScopedGitHubQuery(emptyQueries, queries)
    ) {
      warn(
        'ghScopedZeroUnproven',
        'GitHub code search returned no results for a scoped repository query. Treat this as unproven absence: verify the repo/path with ghViewRepoStructure, then materialize or clone a bounded path and search locally before concluding.'
      );
    }

    // Unlike ghSearchRepos (which GitHub rejects with a 422 for an overly
    // long/complex query), the code-search endpoint has been observed to
    // silently under-match instead of erroring — a genuine absence and a
    // too-complex query both come back as a clean zero-result response. Flag
    // it heuristically so an unexplained empty result with many keywords
    // isn't mistaken for proof of absence.
    const COMPLEX_QUERY_KEYWORD_THRESHOLD = 8;
    const unexplainedComplexEmpty = emptyQueries.filter(
      ({ id, nonExistentScope, incompleteResults }) => {
        if (nonExistentScope || incompleteResults) return false;
        const kws = (queriesById.get(id) as { keywords?: unknown } | undefined)
          ?.keywords;
        return (
          Array.isArray(kws) && kws.length > COMPLEX_QUERY_KEYWORD_THRESHOLD
        );
      }
    );
    if (unexplainedComplexEmpty.length > 0) {
      warn(
        'ghQueryPossiblyTooComplex',
        `Quer${unexplainedComplexEmpty.length > 1 ? 'ies' : 'y'} ${unexplainedComplexEmpty.map(q => q.id).join(', ')} used more than ${COMPLEX_QUERY_KEYWORD_THRESHOLD} keywords and returned zero results. GitHub code search can silently under-match an overly long/complex query instead of erroring — narrow to fewer, more specific keywords before concluding absence.`
      );
    }

    // Repo-state disambiguation for scoped-zero queries: say WHY it was empty
    // (renamed / archived / gone) and hand a corrected retry when renamed.
    for (const { id, state, query } of repoStates) {
      if (state.kind === 'renamed') {
        const [newOwner, newRepo] = state.fullName.split('/');
        warn(
          'ghRepoRenamed',
          `Query ${id}: the repository was RENAMED to ${state.fullName} — searches against the old name silently miss. Retry with the new name (see next.retryRenamed).`
        );
        const kws = (query as { keywords?: unknown } | undefined)?.keywords;
        responseData.next = {
          ...(responseData.next ?? {}),
          [`retryRenamed:${id}`]: {
            tool: 'ghSearchCode',
            query: {
              owner: newOwner,
              repo: newRepo,
              ...(Array.isArray(kws) ? { keywords: kws } : {}),
            },
            why: 'Re-run the same search against the renamed repository',
            confidence: 'exact',
          },
        };
      } else if (state.kind === 'archived') {
        warn(
          'ghRepoArchived',
          `Query ${id}: the repository is ARCHIVED — the code-search index may lag or exclude it; zero matches is not proof of absence. Materialize the repo and search locally to verify.`
        );
      } else {
        warn(
          'ghRepoNotFound',
          `Query ${id}: the repository was NOT FOUND — it does not exist (or is private to this token). Check the owner/repo spelling.`,
          'error'
        );
      }
    }
    if (diagnostics.length > 0) responseData.diagnostics = diagnostics;

    return formatFinalizedResponse<GitHubCodeSearchOutputLocal>(
      responseData,
      [
        'results',
        'id',
        'data',
        'files',
        'path',
        'owner',
        'repo',
        'queryId',
        'matches',
        'value',
        'pathOnly',
        'matchIndices',
        'pagination',
        'next',
        'tool',
        'query',
        'why',
        'confidence',
        'emptyQueries',
        'nonExistentScope',
        'incompleteResults',
        'warnings',
        'errors',
      ],
      groups.length === 0 && errors.length > 0
    );
  };
}
