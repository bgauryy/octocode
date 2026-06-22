import { compileWhere } from '../../adapters/compile.js';
import { diagnostic } from '../../diagnostics.js';
import type { OqlQuery } from '../../types.js';
import type { TransformResult } from '../types.js';
import { toGithubCodeLanguageParams } from '../language.js';
import {
  firstScopeLanguage,
  firstScopePath,
  requestedRowLimit,
  splitGithubSource,
} from './common.js';

export type GithubCodeSearchToolQuery = Record<string, unknown>;

export type GithubCodeSearchTransformOptions = {
  defaultMatch?: 'file' | 'path';
  unsupportedMessage?: string;
  unsupportedBackend?: string;
};

export function toGithubCodeSearchToolQuery(
  query: OqlQuery,
  options: GithubCodeSearchTransformOptions = {}
): TransformResult<GithubCodeSearchToolQuery> {
  if (!query.where) {
    return {
      ok: false,
      diagnostics: [
        diagnostic(
          'requiresMaterialization',
          options.unsupportedMessage ??
            'GitHub code search needs a positive code predicate.',
          { backend: options.unsupportedBackend ?? 'ghSearchCode' }
        ),
      ],
    };
  }

  const compiled = compileWhere(query.where);
  if (
    compiled.unsupported ||
    compiled.negate ||
    compiled.match?.mode === 'structural'
  ) {
    return {
      ok: false,
      diagnostics: [
        diagnostic(
          'requiresMaterialization',
          options.unsupportedMessage ??
            'This predicate cannot be evaluated by GitHub code search; materialize for local proof.',
          { backend: options.unsupportedBackend ?? 'ghSearchCode' }
        ),
      ],
    };
  }

  const { owner, repo } = splitGithubSource(query.from);
  const params = query.params ?? {};
  const languageParams =
    typeof params.extension === 'string'
      ? {}
      : toGithubCodeLanguageParams(firstScopeLanguage(query.scope));
  const limit = requestedRowLimit(query);
  const match =
    typeof params.match === 'string' ? params.match : options.defaultMatch;

  return {
    ok: true,
    diagnostics: [],
    query: {
      ...(owner ? { owner } : {}),
      ...(repo ? { repo } : {}),
      keywords: [compiled.match?.keywords ?? ''],
      ...languageParams,
      ...(firstScopePath(query.scope)
        ? { path: firstScopePath(query.scope) }
        : {}),
      ...(match ? { match } : {}),
      ...(typeof params.concise === 'boolean'
        ? { concise: params.concise }
        : {}),
      ...(typeof params.extension === 'string'
        ? { extension: params.extension }
        : {}),
      ...(typeof params.filename === 'string'
        ? { filename: params.filename }
        : {}),
      ...(limit ? { limit } : {}),
      ...(query.page ? { page: query.page } : {}),
    },
  };
}
