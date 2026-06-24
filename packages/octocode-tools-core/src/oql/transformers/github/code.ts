import { compileWhere } from '../../adapters/compile.js';
import { diagnostic } from '../../diagnostics.js';
import type { OqlDiagnostic, OqlQuery, Predicate } from '../../types.js';
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

/**
 * Path-like field predicates (`basename`/`extension`/`path` with op `=`) map
 * directly onto GitHub code-search path qualifiers — the same provider route
 * OQL target:"files" uses. Returns the ghSearchCode param
 * fragment, or `null` if the predicate is not a provider-expressible path-field
 * equality (those still require materialization, per the planner files lane).
 */
export function githubPathFieldParams(
  where: Predicate
): GithubCodeSearchToolQuery | null {
  if (where.kind !== 'field' || where.op !== '=') return null;
  if (typeof where.value !== 'string' || where.value.length === 0) return null;
  switch (where.field) {
    case 'basename':
      return { filename: where.value, match: 'path' };
    case 'extension':
      return { extension: where.value.replace(/^\./, ''), match: 'path' };
    case 'path':
      return { keywords: [where.value], match: 'path' };
    default:
      return null;
  }
}

export function toGithubCodeSearchToolQuery(
  query: OqlQuery,
  options: GithubCodeSearchTransformOptions = {}
): TransformResult<GithubCodeSearchToolQuery> {
  if (!query.where) {
    return {
      ok: false,
      diagnostics: [
        diagnostic(
          'vendorNoEquivalent',
          options.unsupportedMessage ??
            'GitHub code search needs a positive code predicate.',
          { backend: options.unsupportedBackend ?? 'ghSearchCode' }
        ),
      ],
    };
  }

  // Path-like field equality (basename/extension/path) compiles to provider
  // path qualifiers rather than ripgrep keywords, so it bypasses compileWhere.
  const pathField = githubPathFieldParams(query.where);
  if (pathField) {
    const lossyDiagnostics = githubCodeLossyScopeDiagnostics(query, options);
    if (lossyDiagnostics.length > 0) {
      return { ok: false, diagnostics: lossyDiagnostics };
    }
    const { owner, repo } = splitGithubSource(query.from);
    const params = query.params ?? {};
    const languageParams =
      typeof params.extension === 'string' ||
      typeof pathField.extension === 'string'
        ? {}
        : toGithubCodeLanguageParams(firstScopeLanguage(query.scope));
    const limit = requestedRowLimit(query);
    const scopePath = firstScopePath(query.scope);
    return {
      ok: true,
      diagnostics: [],
      query: {
        ...(owner ? { owner } : {}),
        ...(repo ? { repo } : {}),
        ...languageParams,
        ...pathField,
        ...(scopePath ? { path: scopePath } : {}),
        ...(typeof params.extension === 'string'
          ? { extension: params.extension }
          : {}),
        ...(limit ? { limit } : {}),
        ...(query.page ? { page: query.page } : {}),
      },
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
          'unsupportedVendorPredicate',
          compiled.unsupported?.message ??
            options.unsupportedMessage ??
            'This predicate cannot be evaluated by GitHub code search; materialize for local proof.',
          {
            backend: options.unsupportedBackend ?? 'ghSearchCode',
            ...(compiled.unsupported?.predicateId
              ? { predicateId: compiled.unsupported.predicateId }
              : {}),
          }
        ),
      ],
    };
  }

  const lossyDiagnostics = githubCodeLossyScopeDiagnostics(query, options);
  if (lossyDiagnostics.length > 0) {
    return {
      ok: false,
      diagnostics: lossyDiagnostics,
    };
  }

  // GitHub code search needs a non-empty term. An empty/whitespace keyword
  // (e.g. text "") would otherwise yield `keywords:['']` and silently match
  // nothing while reporting ok — surface it as an unrepresentable predicate.
  const keyword = compiled.match?.keywords ?? '';
  if (keyword.trim().length === 0) {
    return {
      ok: false,
      diagnostics: [
        diagnostic(
          'vendorNoEquivalent',
          'GitHub code search needs a non-empty search term.',
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
      keywords: [keyword],
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

function githubCodeLossyScopeDiagnostics(
  query: OqlQuery,
  options: GithubCodeSearchTransformOptions
): OqlDiagnostic[] {
  const diagnostics: OqlDiagnostic[] = [];
  const backend = options.unsupportedBackend ?? 'ghSearchCode';

  if (Array.isArray(query.scope?.language) && query.scope.language.length > 1) {
    diagnostics.push(
      diagnostic(
        'lossyTransform',
        'GitHub code search cannot express multiple scope.language values without dropping values; materialize for local proof.',
        { backend, queryPath: 'scope.language' }
      )
    );
  }

  if (Array.isArray(query.scope?.path) && query.scope.path.length > 1) {
    diagnostics.push(
      diagnostic(
        'lossyTransform',
        'GitHub code search cannot express multiple scope.path values without dropping values; materialize for local proof.',
        { backend, queryPath: 'scope.path' }
      )
    );
  }

  return diagnostics;
}
