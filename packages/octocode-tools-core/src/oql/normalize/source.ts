/* ------------------------------ source ---------------------------------- */
import { diagnostic } from '../diagnostics.js';
import {
  CORPUS_OPTIONAL_TARGETS,
  type OqlInputQuery,
  type OqlQuery,
  type QuerySource,
} from '../types.js';
import { fail } from './shared.js';

export function normalizeSource(
  raw: OqlInputQuery,
  target: OqlQuery['target']
): QuerySource | undefined {
  const explicitFrom = raw.from;
  const hasRepoSugar =
    typeof raw.repo === 'string' || typeof raw.owner === 'string';
  const topPath = raw.path;

  if (explicitFrom) {
    if (hasRepoSugar) {
      fail(
        diagnostic(
          'ambiguousSugar',
          'Provide either `from` or top-level repo/owner sugar, not both.',
          { queryPath: 'from' }
        )
      );
    }
    return normalizeGithubIdentity(explicitFrom);
  }

  if (hasRepoSugar) {
    const owner = typeof raw.owner === 'string' ? raw.owner : undefined;
    let repo = typeof raw.repo === 'string' ? raw.repo : undefined;
    if (owner && repo && !repo.includes('/')) {
      repo = `${owner}/${repo}`;
    }
    const src: QuerySource = { kind: 'github' };
    if (repo) src.repo = repo;
    if (owner && !repo) src.owner = owner;
    if (typeof raw.ref === 'string') src.ref = raw.ref;
    return src;
  }

  // no repo, no explicit from
  if (typeof topPath === 'string') {
    return { kind: 'local', path: topPath };
  }
  if (Array.isArray(topPath) && typeof topPath[0] === 'string') {
    // OQL accepts one canonical corpus root. If legacy callers pass multiple
    // roots, normalization keeps the first and ignores the rest.
    return { kind: 'local', path: topPath[0] };
  }

  // packages discovery defaults to the npm registry corpus.
  if (target === 'packages') return { kind: 'npm' };
  // repositories discovery may be provider-wide (no concrete repo).
  if (target === 'repositories') return { kind: 'github' };
  // other corpus-optional targets simply have no corpus.
  if (CORPUS_OPTIONAL_TARGETS.includes(target)) return undefined;

  fail(
    diagnostic(
      'invalidQuery',
      'A corpus is required: provide `from`, a `repo`, or a local `path`.',
      { queryPath: 'from' }
    )
  );
}

function normalizeGithubIdentity(from: QuerySource): QuerySource {
  if (from.kind !== 'github') return from;
  const owner = from.owner;
  let repo = from.repo;
  if (owner && repo && !repo.includes('/')) {
    repo = `${owner}/${repo}`;
    const next: QuerySource = { kind: 'github', repo };
    if (from.ref) next.ref = from.ref;
    return next;
  }
  return from;
}
