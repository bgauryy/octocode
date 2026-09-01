import type { BulkFinalizer } from '../../../types/bulk.js';
import type { ToolResultMeta } from '../../../types/toolResults.js';
import {
  collectFlatErrors,
  formatFinalizedResponse,
  type QueryWithPagination,
} from '../../../utils/response/groupedFinalizer.js';
import type {
  GitHubCodeSearchData,
  GitHubCodeSearchOutputLocal,
} from '../scheme.js';
import type { RepoState } from '../execution.js';

import { type CodeSearchPagination } from '../../providerMappers.js';
import {
  applyExactMatchRanking,
  hasScopedGitHubQuery,
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
    const paginationByQuery = new Map<number, CodeSearchPagination>();

    const emptyQueries: Array<{
      index: number;
      nonExistentScope?: true;
      incompleteResults?: true;
    }> = [];
    const incompleteQueryIndexes = new Set<number>();

    const repoStates: Array<{
      index: number;
      state: RepoState;
      query: QueryWithPagination | undefined;
    }> = [];
    results.forEach(res => {
      if (res.status === 'error') return;

      const flat = readPerQueryFlat(res);
      if (flat.repoState) {
        repoStates.push({
          index: res.index,
          state: flat.repoState,
          query: queries[res.index],
        });
      }
      if (flat.incompleteResults) incompleteQueryIndexes.add(res.index);
      const totalMatches = flat.results.reduce(
        (sum, group) => sum + group.matches.length,
        0
      );
      if (totalMatches === 0) {
        emptyQueries.push({
          index: res.index,
          ...(flat.nonExistentScope ? { nonExistentScope: true as const } : {}),
          ...(flat.incompleteResults
            ? { incompleteResults: true as const }
            : {}),
        });
      }
      const groups = flat.results;
      perQueryGroups.push({ index: res.index, groups });

      if (flat.pagination) {
        paginationByQuery.set(res.index, flat.pagination);
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
    const sourceRows = new Map(results.map(row => [row.index, row]));
    const successRecords = buildResultRecords(
      queries,
      groups,
      paginationByQuery
    ).map(record => {
      const meta = sourceRows.get(record.index)?.meta;
      return { ...record, ...(meta ? { meta } : {}) };
    });
    const nextMap = buildNextMap(successRecords, queries, allKeywords);
    if (nextMap) {
      for (const record of successRecords) {
        const key =
          successRecords.length === 1 ? 'getLines' : `getLines:${record.index}`;
        const continuation = nextMap[key];
        if (continuation) {
          (record.data as unknown as GitHubCodeSearchData).next = {
            getLines: continuation,
          };
        }
      }
    }
    if (conciseMode) {
      for (const rec of successRecords) {
        rec.data.files = rec.data.files.map(
          f => `${f.owner}/${f.repo}:${f.path}`
        ) as unknown as typeof rec.data.files;
      }
    }

    const emptyRecords = emptyQueries.map(
      ({ index, nonExistentScope, incompleteResults }) => {
        const meta = sourceRows.get(index)?.meta;
        return {
          index,
          status: 'empty' as const,
          ...(meta ? { meta } : {}),
          data: {
            files: [],
            ...(nonExistentScope ? { nonExistentScope } : {}),
            ...(incompleteResults ? { incompleteResults } : {}),
          },
        };
      }
    );
    const errorRecords = errors.map(({ index, error }) => {
      const meta = sourceRows.get(index)?.meta;
      return {
        index,
        status: 'error' as const,
        ...(meta ? { meta } : {}),
        data: { error },
      };
    });
    const resultRecords = [
      ...successRecords,
      ...emptyRecords,
      ...errorRecords,
    ].sort((left, right) => left.index - right.index);
    const responseData: GitHubCodeSearchOutputLocal = {
      results: resultRecords,
    };

    type AgentRow = {
      index: number;
      meta?: ToolResultMeta;
      data: GitHubCodeSearchData | { error: string };
    };
    const agentRows = responseData.results as AgentRow[];
    const rowAt = (index: number): AgentRow | undefined =>
      agentRows.find(row => row.index === index);
    const addDiagnostic = (
      index: number,
      code: string,
      hint: string,
      partial = false
    ): void => {
      const row = rowAt(index);
      if (!row) return;
      const previous = row.meta;
      const diagnostics = previous?.diagnostics;
      row.meta = {
        evidence: previous?.evidence ?? {
          kind: 'provider',
          confidence: 'medium',
        },
        diagnostics: {
          ...(diagnostics ?? {}),
          codes: Array.from(new Set([...(diagnostics?.codes ?? []), code])),
          hints: Array.from(new Set([...(diagnostics?.hints ?? []), hint])),
          ...(partial || diagnostics?.partial ? { partial: true } : {}),
        },
      };
    };
    const addContinuation = (
      index: number,
      name: string,
      continuation: NonNullable<GitHubCodeSearchData['next']>[string]
    ): void => {
      const row = rowAt(index);
      if (!row || !('files' in row.data)) return;
      row.data.next = { ...(row.data.next ?? {}), [name]: continuation };
    };

    for (const index of incompleteQueryIndexes) {
      addDiagnostic(
        index,
        'ghIncompleteResults',
        'GitHub reported an incomplete search index result; retry, narrow the scope, or verify locally before concluding absence.',
        true
      );
      const {
        goal: _goal,
        reasoning: _reasoning,
        ...retryQuery
      } = queries[index] as QueryWithPagination & Record<string, unknown>;
      addContinuation(index, 'retry', {
        tool: 'github.code',
        query: retryQuery,
        why: 'Retry the same query because GitHub marked the result incomplete',
        confidence: 'exact',
      });
    }

    const repoStateIndexes = new Set(repoStates.map(({ index }) => index));
    for (const empty of emptyQueries) {
      if (
        repoStateIndexes.has(empty.index) ||
        !hasScopedGitHubQuery([empty], queries)
      ) {
        continue;
      }
      const query = queries[empty.index] as QueryWithPagination & {
        owner: string;
        repo: string;
      };
      addDiagnostic(
        empty.index,
        'ghScopedZeroUnproven',
        'No indexed matches is unproven absence; verify the repository structure and search a bounded local copy before concluding.'
      );
      addContinuation(empty.index, 'viewStructure', {
        tool: 'github.tree',
        query: { owner: query.owner, repo: query.repo, path: '' },
        why: 'Verify that the scoped repository and path exist before concluding absence',
        confidence: 'exact',
      });
    }

    // Complex code queries can silently under-match instead of returning 422.
    const COMPLEX_QUERY_KEYWORD_THRESHOLD = 8;
    const unexplainedComplexEmpty = emptyQueries.filter(
      ({ index, nonExistentScope, incompleteResults }) => {
        if (nonExistentScope || incompleteResults) return false;
        const kws = (queries[index] as { keywords?: unknown } | undefined)
          ?.keywords;
        return (
          Array.isArray(kws) && kws.length > COMPLEX_QUERY_KEYWORD_THRESHOLD
        );
      }
    );
    if (unexplainedComplexEmpty.length > 0) {
      for (const { index } of unexplainedComplexEmpty) {
        const query = queries[index] as QueryWithPagination & {
          keywords?: unknown;
          owner?: unknown;
          repo?: unknown;
          path?: unknown;
          filename?: unknown;
          extension?: unknown;
          language?: unknown;
          match?: unknown;
        };
        const keywords = Array.isArray(query.keywords)
          ? query.keywords
              .filter(
                (keyword): keyword is string => typeof keyword === 'string'
              )
              .slice(0, COMPLEX_QUERY_KEYWORD_THRESHOLD)
          : [];
        addDiagnostic(
          index,
          'ghQueryPossiblyTooComplex',
          `The query used more than ${COMPLEX_QUERY_KEYWORD_THRESHOLD} keywords; narrow it before treating zero matches as absence.`
        );
        addContinuation(index, 'retryNarrow', {
          tool: 'github.code',
          query: {
            keywords,
            ...(typeof query.owner === 'string' ? { owner: query.owner } : {}),
            ...(typeof query.repo === 'string' ? { repo: query.repo } : {}),
            ...(typeof query.path === 'string' ? { path: query.path } : {}),
            ...(typeof query.filename === 'string'
              ? { filename: query.filename }
              : {}),
            ...(typeof query.extension === 'string'
              ? { extension: query.extension }
              : {}),
            ...(typeof query.language === 'string'
              ? { language: query.language }
              : {}),
            ...(query.match === 'file' || query.match === 'path'
              ? { match: query.match }
              : {}),
          },
          why: 'Retry with a bounded keyword set to avoid silent under-matching',
          confidence: 'low',
        });
      }
    }

    // Disambiguate scoped-zero results and attach the exact recovery path.
    for (const { index, state, query } of repoStates) {
      if (state.kind === 'renamed') {
        const [newOwner, newRepo] = state.fullName.split('/');
        addDiagnostic(
          index,
          'ghRepoRenamed',
          `The repository was renamed to ${state.fullName}; retry against the renamed repository.`
        );
        const kws = (query as { keywords?: unknown } | undefined)?.keywords;
        addContinuation(index, 'retryRenamed', {
          tool: 'github.code',
          query: {
            owner: newOwner,
            repo: newRepo,
            ...(Array.isArray(kws) ? { keywords: kws } : {}),
          },
          why: 'Re-run the same search against the renamed repository',
          confidence: 'exact',
        });
      } else if (state.kind === 'archived') {
        addDiagnostic(
          index,
          'ghRepoArchived',
          'The repository is archived, so its code-search index may lag or be incomplete; verify its structure and search locally.'
        );
        const scoped = query as
          | (QueryWithPagination & { owner?: unknown; repo?: unknown })
          | undefined;
        if (
          typeof scoped?.owner === 'string' &&
          typeof scoped.repo === 'string'
        ) {
          addContinuation(index, 'viewStructure', {
            tool: 'github.tree',
            query: { owner: scoped.owner, repo: scoped.repo, path: '' },
            why: 'Inspect the archived repository outside the code-search index',
            confidence: 'exact',
          });
        }
      } else {
        addDiagnostic(
          index,
          'ghRepoNotFound',
          'The repository was not found or is private to this token; verify the spelling or discover the current repository name.'
        );
        const scoped = query as
          (QueryWithPagination & { repo?: unknown }) | undefined;
        if (typeof scoped?.repo === 'string') {
          addContinuation(index, 'findRepository', {
            tool: 'github.repositories',
            query: { keywords: [scoped.repo] },
            why: 'Find the repository by name in case it moved or was renamed',
            confidence: 'low',
          });
        }
      }
    }

    return formatFinalizedResponse<GitHubCodeSearchOutputLocal>(
      responseData,
      [
        'results',
        'index',
        'status',
        'meta',
        'evidence',
        'diagnostics',
        'data',
        'files',
        'path',
        'owner',
        'repo',
        'queryIndex',
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
        'nonExistentScope',
        'incompleteResults',
        'error',
      ],
      groups.length === 0 && errors.length > 0
    );
  };
}
